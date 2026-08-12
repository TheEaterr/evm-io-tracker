use ethers::types::{ExecutedInstruction, Opcode, VMOperation, U256};
use std::ops::Div;

pub fn pop_num(op: &Opcode) -> usize {
    use Opcode::*;
    match &op {
        STOP => 0,
        ADD | MUL | SUB | DIV | SDIV | MOD | SMOD | EXP | SIGNEXTEND => 2,
        ADDMOD | MULMOD => 3,
        ISZERO | NOT | CLZ => 1,
        LT | GT | SLT | SGT | EQ | AND | OR | XOR | BYTE | SHL | SHR | SAR | SHA3 => 2,
        BALANCE | CALLDATALOAD | EXTCODESIZE | EXTCODEHASH => 1,
        CALLDATACOPY | CODECOPY | RETURNDATACOPY => 3,
        EXTCODECOPY => 4,
        ADDRESS | ORIGIN | CALLER | CALLVALUE | CALLDATASIZE | CODESIZE | GASPRICE
        | RETURNDATASIZE => 0,
        BLOCKHASH => 1,
        COINBASE | TIMESTAMP | NUMBER | DIFFICULTY | GASLIMIT | CHAINID | SELFBALANCE | BASEFEE => {
            0
        }
        POP | MLOAD | SLOAD | TLOAD | JUMP => 1,
        MSTORE | MSTORE8 | SSTORE | TSTORE | JUMPI => 2,
        MCOPY => 3,
        PC | MSIZE | GAS | JUMPDEST => 0,
        PUSH0 | PUSH1 | PUSH2 | PUSH3 | PUSH4 | PUSH5 | PUSH6 | PUSH7 | PUSH8 | PUSH9 | PUSH10 | PUSH11
        | PUSH12 | PUSH13 | PUSH14 | PUSH15 | PUSH16 | PUSH17 | PUSH18 | PUSH19 | PUSH20
        | PUSH21 | PUSH22 | PUSH23 | PUSH24 | PUSH25 | PUSH26 | PUSH27 | PUSH28 | PUSH29
        | PUSH30 | PUSH31 | PUSH32 => 0,
        DUP1 => 1,
        DUP2 => 2,
        DUP3 => 3,
        DUP4 => 4,
        DUP5 => 5,
        DUP6 => 6,
        DUP7 => 7,
        DUP8 => 8,
        DUP9 => 9,
        DUP10 => 10,
        DUP11 => 11,
        DUP12 => 12,
        DUP13 => 13,
        DUP14 => 14,
        DUP15 => 15,
        DUP16 => 16,
        SWAP1 => 2,
        SWAP2 => 3,
        SWAP3 => 4,
        SWAP4 => 5,
        SWAP5 => 6,
        SWAP6 => 7,
        SWAP7 => 8,
        SWAP8 => 9,
        SWAP9 => 10,
        SWAP10 => 11,
        SWAP11 => 12,
        SWAP12 => 13,
        SWAP13 => 14,
        SWAP14 => 15,
        SWAP15 => 16,
        SWAP16 => 17,
        LOG0 => 2,
        LOG1 => 3,
        LOG2 => 4,
        LOG3 => 5,
        LOG4 => 6,
        CREATE => 3,
        CALL => 7,
        CALLCODE => 7,
        RETURN => 2,
        DELEGATECALL => 6,
        CREATE2 => 4,
        STATICCALL => 6,
        REVERT => 2,
        INVALID => 0,
        SELFDESTRUCT => 1,
    }
}

pub fn integrity_check(op: &VMOperation, stack: &[U256], block_number: usize) {
    use Opcode::*;
    let opcode = if let ExecutedInstruction::Known(o) = &op.op {
        o.clone()
    } else {
        return;
    };
    let bool_to_u256 = |b| if b { U256::one() } else { U256::zero() };
    if let ADD | SUB | MUL | DIV | AND | OR | XOR | EQ | LT | GT | EXP = opcode {
        let a = stack[stack.len() - 1];
        let b = stack[stack.len() - 2];
        let expected = match &opcode {
            ADD => a.overflowing_add(b).0,
            SUB => a.overflowing_sub(b).0,
            MUL => a.overflowing_mul(b).0,
            DIV => {
                if b.is_zero() {
                    U256::zero()
                } else {
                    a.div(b)
                }
            }
            AND => a & b,
            OR => a | b,
            XOR => a ^ b,
            EQ => bool_to_u256(a == b),
            LT => bool_to_u256(a < b),
            GT => bool_to_u256(a > b),
            EXP => a.overflowing_pow(b).0,
            _ => unreachable!(),
        };
        let real = op.ex.as_ref().unwrap().push.first().unwrap();
        assert_eq!(
            &expected, real,
            "Integrity check fail at {}: Op {:?}",
            block_number, opcode
        );
    } else if SWAP1 as u8 <= opcode as u8 && opcode as u8 <= SWAP16 as u8 {
        let depth = ((opcode as u8 - SWAP1 as u8) + 2) as usize;
        let poped = &stack[stack.len() - depth..];
        let pushed = &op.ex.as_ref().unwrap().push[..];
        assert_eq!(
            pushed[0],
            poped[depth - 1],
            "Integrity check fail at {}: Op {:?}",
            block_number,
            opcode
        );
        assert_eq!(
            pushed[depth - 1],
            poped[0],
            "Integrity check fail at {}: Op {:?}",
            block_number,
            opcode
        );
        assert_eq!(
            pushed[1..depth - 1],
            poped[1..depth - 1],
            "Integrity check fail at {}: Op {:?}",
            block_number,
            opcode
        );
    } else if DUP1 as u8 <= opcode as u8 && opcode as u8 <= DUP16 as u8 {
        let depth = ((opcode as u8 - DUP1 as u8) + 1) as usize;
        let poped = &stack[stack.len() - depth..];
        let pushed = &op.ex.as_ref().unwrap().push[..];
        assert_eq!(
            pushed[depth], poped[0],
            "Integrity check fail at {}: Op {:?}",
            block_number, opcode
        );
        assert_eq!(
            pushed[0..depth],
            poped[0..depth],
            "Integrity check fail at {}: Op {:?}",
            block_number,
            opcode
        );
    }
}
