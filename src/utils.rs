use ethers::types::{Address, BigEndianHash, H256, U256};

const MIN_BASE_FEE_PER_BLOB_GAS: U256 = U256([1, 0, 0, 0]);
const BLOB_BASE_FEE_UPDATE_FRACTION: U256 = U256([11684671, 0, 0, 0]);

/// Approximate exponential-like series used by the project.
///
/// Computes sum_{i>=0} factor * (numerator/denominator)^i, using integer
/// arithmetic and stopping when additional terms become zero.
pub fn fake_exponential(factor: U256, numerator: U256, denominator: U256) -> U256 {
    if denominator == 0.into() {
        return 0.into();
    }

    // If numerator is zero, only the i=0 term remains: factor * 1
    if numerator == 0.into() {
        return factor;
    }

    let mut i: U256 = 1.into();
    let mut output: U256 = 0.into();
    let mut numerator_accum: U256 = factor.saturating_mul(denominator);

    while numerator_accum > 0.into() {
        output = output.saturating_add(numerator_accum);

        // numerator_accum = (numerator_accum * numerator) / (denominator * i)
        let denom_times_i = denominator.saturating_mul(i);
        if denom_times_i == 0.into() {
            break;
        }
        numerator_accum = (numerator_accum.saturating_mul(numerator)) / denom_times_i;
        i = i.saturating_add(1.into());
    }

    output / denominator
}

pub fn get_base_fee_per_blob_gas(excess_blob_gas: U256) -> U256 {
    fake_exponential(
        MIN_BASE_FEE_PER_BLOB_GAS,
        excess_blob_gas,
        BLOB_BASE_FEE_UPDATE_FRACTION
    )
}

#[inline]
pub fn u256_to_address(value: &U256) -> Address {
    let addr: H256 = BigEndianHash::from_uint(value);
    Address::from(addr)
}

#[inline]
pub fn u256_to_hash(value: &U256) -> H256 {
    BigEndianHash::from_uint(value)
}
