use crate::BlockAccess;

use super::DBAccess;

use super::opcode::{integrity_check, pop_num};
use super::SlotKey;
use ethers::{
    providers::{Http, Middleware, Provider},
    types::{
        Address, BigEndianHash, BlockNumber, ExecutedInstruction, Opcode, TraceType, VMTrace, H256,
        U256,
    },
};

pub async fn parse_block_trace(provider: Provider<Http>, number: usize) -> BlockAccess {
    let mut block_accesses = Vec::new();
    let answer = provider
        .trace_replay_block_transactions(
            BlockNumber::Number(number.into()),
            vec![TraceType::Trace, TraceType::VmTrace],
        )
        .await
        .unwrap();

    let receipts = provider
        .get_block_receipts(BlockNumber::Number(number.into()))
        .await
        .unwrap();
    assert_eq!(answer.len(), receipts.len());
    for (trace, receipt) in answer.into_iter().zip(receipts.into_iter()) {
        let contract = match (receipt.to, receipt.contract_address) {
            (Some(x), None) => x,
            (None, Some(x)) => x,
            (None, None) => {
                continue;
            }
            _ => unreachable!(),
        };
        if let Some(trace) = &trace.vm_trace {
            let mut transaction_access = Vec::new();
            parse_trace(trace, contract, &mut transaction_access, number);
            block_accesses.push((if receipt.contract_address.is_some() {
                super::TransactionType::Contract
            } else {
                super::TransactionType::Regular
            }, transaction_access));
        }
    }
    block_accesses
}

fn parse_trace(
    trace: &VMTrace,
    contract: Address,
    accesses: &mut Vec<DBAccess>,
    block_number: usize,
) {
    use Opcode::*;
    let mut stack: Vec<U256> = vec![];

    for op in trace.ops.iter().filter(|op| op.ex.is_some()) {
        let opcode = match &op.op {
            ExecutedInstruction::Known(o) => o.clone(),
            ExecutedInstruction::Unknown(s) if s == "SHA3" => KECCAK256,
            ExecutedInstruction::Unknown(s) => {
                println!("Unknown opcode: {}", s);
                INVALID
            }
        };
        // println!("stack {:?}", &stack);
        // println!("op {:?}", op);
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
                parse_trace(sub_trace, next_contract, accesses, block_number);
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

        stack.truncate(stack.len() - pop_num(&opcode));
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
