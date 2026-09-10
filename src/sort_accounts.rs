use ethers::types::H160;
use ethers::
    providers::{Http, Middleware, Provider}
;
use ordermap::OrderSet;

pub async fn get_addresses_in_block(provider: Provider<Http>, number: usize) -> OrderSet<H160> {
    let block = provider
        .get_block_with_txs(ethers::types::BlockNumber::Number(number.into()))
        .await
        .unwrap()
        .unwrap();

    let mut new_addresses: OrderSet<H160> = OrderSet::new();

    for tx in block.transactions.iter() {
        if let Some(to) = tx.to {
            new_addresses.insert(to);
            new_addresses.insert(tx.from);
        }
    }
    new_addresses
}