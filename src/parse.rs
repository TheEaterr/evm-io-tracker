use std::collections::HashMap;
use std::str::FromStr;

use crate::opcode::integrity_check;
use crate::{BlockAccess, TransactionInfo};

use super::DBAccess;

use super::opcode::{pop_num};
use super::SlotKey;
use ethers::types::{BlockTrace, TransactionReceipt};
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{
        Address, BigEndianHash, BlockNumber, ExecutedInstruction, Opcode, TraceType,
        TransactionTrace, Action, VMTrace, H256, U256,
    },
};

pub async fn parse_block_trace(provider: Provider<Http>, number: usize) -> BlockAccess {
    let mut block_accesses = Vec::new();

    // Check if the block trace is already saved to file
    let block_trace_path = format!("./data/blocktrace_{}.json", number);
    let receipts_path = format!("./data/receipts_{}.json", number);

    let answer: Vec<BlockTrace> = if std::path::Path::new(&block_trace_path).exists() {
        let an = std::fs::read_to_string(&block_trace_path).unwrap();
        println!("Block trace loaded from file: {}", block_trace_path);
        serde_json::from_str(&an).unwrap()
    } else {
        provider
        .trace_replay_block_transactions(
            BlockNumber::Number(number.into()),
            vec![TraceType::Trace, TraceType::VmTrace],
        )
        .await
        .unwrap()
    };

    let receipts: Vec<TransactionReceipt> = if std::path::Path::new(&receipts_path).exists() {
        let receipts = std::fs::read_to_string(&receipts_path).unwrap();
        println!("Receipts loaded from file: {}", receipts_path);
        serde_json::from_str(&receipts).unwrap()
    } else {
        provider
        .get_block_receipts(BlockNumber::Number(number.into()))
        .await
        .unwrap()
    };

    assert_eq!(answer.len(), receipts.len());
    // Save blocktrace and receipts to file for debugging
    std::fs::write(format!("/mnt/tank/raw_ethereum_data/blocktrace_{}.json", number), serde_json::to_string(&answer).unwrap())
        .unwrap();
    std::fs::write(
        format!("/mnt/tank/raw_ethereum_data/receipts_{}.json", number),
        serde_json::to_string(&receipts).unwrap(),
    )
    .unwrap();
    for (block_trace, receipt) in answer.into_iter().zip(receipts.into_iter()) {
        let contract = match (receipt.to, receipt.contract_address) {
            (Some(x), None) => x,
            (None, Some(x)) => x,
            (None, None) => {
                continue;
            }
            _ => unreachable!(),
        };
        if let Some(trace) = &block_trace.vm_trace {
            let mut transaction_access = Vec::new();
            println!("======= Parsing trace for transaction: {:?}", receipt.transaction_hash);
            let mut transient_storage = HashMap::<U256, U256>::new();
            parse_trace(trace, contract, &mut transaction_access, &mut transient_storage, number, receipt.transaction_hash == H256::from_str("0x904225d4ff46db25a7a1e7e8609528a6c376ba8fcb873d9e5188cfba510eb709").unwrap());
            let transaction_type: crate::TransactionType = if receipt.contract_address.is_some() {
                super::TransactionType::ContractCreation
            } else if block_trace.trace.is_some() && block_trace.trace.as_ref().unwrap().len() == 1 {
                let first_trace: &TransactionTrace = &block_trace.trace.as_ref().unwrap()[0];
                let Action::Call(call) = &first_trace.action else { continue };
                if call.value != U256::zero() {
                    super::TransactionType::Regular
                } else {
                    super::TransactionType::ContractCall
                }
            } else {
                super::TransactionType::ContractCall
            };
            let transaction_info = TransactionInfo {
                type_: transaction_type.clone(),
                to: receipt.to,
                from: receipt.from,
            };
            block_accesses.push((transaction_info, transaction_access));
        }
    }
    block_accesses
}

fn parse_trace(
    trace: &VMTrace,
    contract: Address,
    accesses: &mut Vec<DBAccess>,
    transient_storage: &mut HashMap<U256, U256>,
    block_number: usize,
    print_trace: bool,
) {
    use Opcode::*;
    let mut stack: Vec<U256> = vec![];

    for op in trace.ops.iter().filter(|op| op.ex.is_some()) {
        let opcode = match &op.op {
            ExecutedInstruction::Known(o) => o.clone(),
            ExecutedInstruction::Unknown(s) => {
                println!("Unknown opcode: {}", s);
                INVALID
            }
        };
        if print_trace {
            println!("stack {:?}", &stack);
            println!("op {:?}", op);
        }
        integrity_check(op, &stack, block_number);

        let peek = |x: usize| &stack[stack.len() - x];

        let single_return = || {
            op.ex
                .as_ref()
                .expect("Ex should exist")
                .push
                .first()
                .expect("Return value should not empty")
                .clone()
        };

        if let Some(sub_trace) = &op.sub {
            if let Some(next_contract) = match opcode {
                CALL | STATICCALL => Some(u256_to_address(peek(2))),
                CALLCODE | DELEGATECALL => Some(contract.clone()),
                CREATE | CREATE2 => Some(u256_to_address(&single_return())),
                _ => None,
            } {
                parse_trace(sub_trace, next_contract, accesses, transient_storage, block_number, print_trace);
            }
        }

        let maybe_access = match &opcode {
            SLOAD => Some(DBAccess::Read(
                SlotKey {
                    address: contract,
                    slot: peek(1).clone(),
                },
                single_return(),
            )),
            SSTORE => Some(DBAccess::Write(
                SlotKey {
                    address: contract,
                    slot: peek(1).clone(),
                },
                peek(2).clone(),
            )),
            _ => None,
        };
        if let Some(access) = maybe_access {
            // println!("{:?}", access);
            accesses.push(access);
        }

        if matches!(&op.op, ExecutedInstruction::Known(TSTORE)) {
            let slot = peek(1).clone();
            let value = peek(2).clone();
            transient_storage.insert(slot, value);
            // println!("TSTORE: slot: {:?}, value: {:?}", slot, value);
        }

        if matches!(&op.op, ExecutedInstruction::Known(TLOAD)) {
            let slot = peek(1).clone();
            let value = transient_storage.get(&slot).cloned().unwrap_or_default();
            stack.truncate(stack.len() - pop_num(&opcode));
            stack.push(value);
            // println!("TLOAD: slot: {:?}, value: {:?}", slot, value);
        } else if matches!(&op.op, ExecutedInstruction::Known(CLZ)) {
            let value = peek(1).clone();
            let leading_zeros = value.leading_zeros();
            let result = U256::from(leading_zeros);
            stack.truncate(stack.len() - pop_num(&opcode));
            stack.push(result);
        } else {
            stack.truncate(stack.len() - pop_num(&opcode));
        }

        let pushed = op.ex.as_ref().map(|x| &x.push);
        stack.extend(pushed.unwrap_or(&vec![]));

        if pushed.map_or(true, Vec::is_empty)
            && matches!(
                &op.op,
                ExecutedInstruction::Known(
                    CALL | CALLCODE | DELEGATECALL | STATICCALL | CREATE | CREATE2
                )
            )
        {
            // println!("Incorrect return of {:?}", &op.op);
            stack.push(U256::zero());
        }
        // println!("{:?}\n", stack);
    }
}

#[inline]
fn u256_to_address(value: &U256) -> Address {
    let addr: H256 = BigEndianHash::from_uint(value);
    Address::from(addr)
}
