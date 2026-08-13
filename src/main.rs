mod opcode;
mod parse;
mod utils;

use opts::{CombineOptions, FetchOptions, SealOptions};
use parse::parse_block_trace;

use ethers::{
    providers::{Http, Provider}, types::{Address, H160, U256}
};
use postcard::{from_bytes, to_stdvec};
use rand::seq::SliceRandom;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
};
use std::{io::Write, path::Path};
use structopt::StructOpt;
use tiny_keccak::{Hasher, Keccak};
use tokio::{task::JoinSet, time::Instant};

use crate::opts::Options;
mod opts;

#[derive(Default, Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct SlotKey {
    pub address: Address,
    pub slot: U256,
}

impl SlotKey {
    fn digest(&self) -> [u8; 32] {
        let mut output = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(self.address.as_ref());
        let mut encoded = [0u8; 32];
        self.slot.to_big_endian(&mut encoded);
        hasher.update(&encoded);
        hasher.finalize(&mut output);
        output
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DBAccess {
    Read(SlotKey, U256),
    Write(SlotKey, U256),
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum TransactionType {
    ContractCreation,
    ContractCall,
    Regular,
}

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct TransactionInfo {
    pub type_: TransactionType,
    pub to: Option<Address>,
    pub from: Address,
}

pub type TransactionAccess = (TransactionInfo, Vec<DBAccess>);

pub type BlockAccess = Vec<TransactionAccess>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ExperimentTask {
    Read([u8; 32]),
    Write([u8; 32], Vec<u8>),
}

async fn fetch_main(opts: &FetchOptions) {
    let number = opts.start_block;
    let batch_size = opts.batch_size;
    let trace_path = opts.trace_path.clone();
    let raw_data_path = opts.raw_data_path.clone();
    let dump_raw_data = opts.dump_raw_data;
    
    if !Path::new(&trace_path).exists() {
        fs::create_dir_all(&trace_path).expect("Failed to create trace_path directory");
    }
    let trace_path = if trace_path.ends_with('/') {
        trace_path
    } else {
        format!("{}/", trace_path)
    };
    let raw_data_path = if raw_data_path.ends_with('/') {
        raw_data_path
    } else {
        format!("{}/", raw_data_path)
    };

    let provider = Provider::<Http>::try_from(opts.node_url.clone())
        .expect("could not instantiate HTTP Provider");

    let mut set = JoinSet::new();

    let start = Instant::now();
    let mut answers: Vec<BlockAccess> = vec![Default::default(); batch_size];
    for x in 0..batch_size {
        let provider = provider.clone();
        let number = number + x;
        println!("Spawning task for block number: {}", number);
        let raw_data_path = raw_data_path.clone();
        set.spawn(async move { (parse_block_trace(provider, raw_data_path, dump_raw_data, number).await, x) });
    }

    let mut accesses_cnt: usize = 0;
    while let Some(results) = set.join_next().await {
        let (accesses, x) = results.unwrap();
        accesses_cnt += accesses.iter().map(|x| x.1.len()).sum::<usize>();
        answers[x] = accesses;
    }

    write_to_file(&answers, format!("{}{}_{}.trace", trace_path, number, batch_size));
    let elapsed = start.elapsed();

    println!(
        "Block number {} to {}: {} items ({:?})",
        number,
        number + batch_size - 1,
        accesses_cnt,
        elapsed
    );

    std::mem::drop(provider);
    // sleep(std::cmp::min(elapsed / 3, Duration::from_secs(10))).await;
}

fn combine(opts: &CombineOptions) {
    let re = Regex::new(r"^(\d+)_(\d+)\.trace$").unwrap();
    let mut pathes: Vec<_> = fs::read_dir(&opts.path)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let path = entry.path();
            if let Some(file_name) = path.file_name() {
                if let Some(captures) = re.captures(file_name.to_str().unwrap()) {
                    let start_number = captures[1].parse::<usize>().unwrap();
                    let length = captures[2].parse::<usize>().unwrap();

                    let start_cond = opts
                        .start_block
                        .map_or(true, |start| start_number + length > start);
                    let end_cond = opts.end_block.map_or(true, |end| start_number < end);
                    if start_cond && end_cond {
                        return Some((path, start_number, length));
                    }
                }
            }
            None
        })
        .collect();
    assert!(pathes.len() > 0, "Not found traces");
    pathes.sort_unstable_by_key(|(_, number, _)| *number);
    for i in 0..(pathes.len() - 1) {
        assert_eq!(
            pathes[i].1 + pathes[i].2,
            pathes[i + 1].1,
            "Provided files are not consecutive ranges: {} -> {}.",
            pathes[i].0.display(),
            pathes[i + 1].0.display()
        );
    }

    let mut combined_answer: Vec<BlockAccess> = Vec::new();
    let actual_start = opts.start_block.unwrap_or(pathes.first().unwrap().1);

    for (path, start_number, _) in pathes {
        let block_access_group: Vec<BlockAccess> = read_from_file(path);
        for (idx, block_access) in block_access_group.into_iter().enumerate() {
            let current_block = start_number + idx;
            if opts
                .start_block
                .map_or(true, |start| current_block >= start)
                && opts.end_block.map_or(true, |end| current_block < end)
            {
                combined_answer.push(block_access);
            }
        }
    }

    let output = Path::new(&opts.path).join(format!(
        "combined_{}_{}.trace",
        actual_start,
        combined_answer.len()
    ));

    write_to_file(&combined_answer, output);
}

fn write_to_file<T: Serialize, S: AsRef<Path>>(data: &T, path: S) {
    let raw = to_stdvec(data).unwrap();
    File::create(path.as_ref())
        .unwrap()
        .write_all(&raw)
        .unwrap();
}

fn read_from_file<T, S: AsRef<Path>>(path: S) -> T
where
    for<'a> T: Deserialize<'a>,
{
    let loaded = std::fs::read(path).unwrap();
    from_bytes(&loaded).unwrap()
}

fn u256_to_bytes(number: &U256) -> Vec<u8> {
    let mut encoded = [0u8; 32];
    number.to_big_endian(&mut encoded);
    encoded.to_vec()
}

fn seal(opts: &SealOptions) {
    let loaded = std::fs::read(&opts.input).unwrap();
    let answer: Vec<BlockAccess> = from_bytes(&loaded).unwrap();

    let mut frontier = HashMap::<SlotKey, U256>::new();
    let mut touched = HashSet::<SlotKey>::new();
    let mut access_stat = HashMap::<SlotKey, (usize, usize, usize)>::new(); // (reads, writes, updates)
    let mut written_slots = HashSet::<SlotKey>::new();

    for tx in answer.iter().flatten() {
        for x in &tx.1 {
            match x {
                DBAccess::Read(slot, value) if !touched.contains(&slot) => {
                    touched.insert(slot.clone());
                    if !value.is_zero() {
                        frontier.insert(slot.clone(), value.clone());
                    }
                    access_stat.entry(slot.clone()).or_default().0 += 1;
                }
                DBAccess::Read(slot, _) => {
                    access_stat.entry(slot.clone()).or_default().0 += 1;
                }
                DBAccess::Write(slot, _) => {
                    touched.insert(slot.clone());
                    if written_slots.contains(&slot) {
                        access_stat.entry(slot.clone()).or_default().2 += 1; // update
                    } else {
                        written_slots.insert(slot.clone());
                        access_stat.entry(slot.clone()).or_default().1 += 1; // write
                        access_stat.entry(slot.clone()).or_default().2 += 1; // update
                    }
                }
            }
        }
        // add the from and to db operations
        for addr in [tx.0.from, tx.0.to.unwrap_or_default()] {
            if addr == Address::zero() {
                continue;
            }
            let slot = SlotKey {
                address: addr,
                slot: U256::zero(),
            };
            if !touched.contains(&slot) {
                written_slots.insert(slot.clone());
                touched.insert(slot.clone());
                access_stat.entry(slot.clone()).or_default().0 += 1;
                access_stat.entry(slot.clone()).or_default().1 += 1;
                frontier.insert(slot.clone(), U256::zero());
            } else {
                access_stat.entry(slot.clone()).or_default().0 += 1;
                access_stat.entry(slot.clone()).or_default().2 += 1; // update
            }
        }
        
    }

    println!(
        "Blocks {}, txs {}, regular {}, contract creations {}, contract calls {}, ops {}",
        answer.len(),
        answer.iter().map(|x| x.len()).sum::<usize>(),
        answer.iter().flatten().filter(|x| (**x).0.type_ == TransactionType::Regular).count(),
        answer.iter().flatten().filter(|x| (**x).0.type_ == TransactionType::ContractCreation).count(),
        answer.iter().flatten().filter(|x| (**x).0.type_ == TransactionType::ContractCall).count(),
        answer.iter().flatten().map(|x| x.1.len()).sum::<usize>(),
    );
    println!("Touched set {}, init set {}", touched.len(), frontier.len());

    // Calculate access distribution
    let mut read_distribution = HashMap::<usize, usize>::new();
    let mut write_distribution = HashMap::<usize, usize>::new();
    let mut update_distribution = HashMap::<usize, usize>::new();
    for (_, (reads, _writes, updates)) in &access_stat {
        *read_distribution.entry(*reads).or_insert(0) += 1;
        *write_distribution.entry(*_writes).or_insert(0) += 1;
        *update_distribution.entry(*updates).or_insert(0) += 1;
    }

    let mut read_vec: Vec<_> = read_distribution.iter().collect();
    read_vec.sort_unstable_by_key(|(count, _)| *count);
    println!("\nRead distribution (read_count -> num_keys):");
    for (count, num_keys) in read_vec {
        if (*count) > 0 {
            println!("  {} reads: {} keys", count, num_keys);
        }
    }

    
    let mut update_vec: Vec<_> = update_distribution.iter().collect();
    update_vec.sort_unstable_by_key(|(count, _)| *count);
    println!("\nUpdate distribution (update_count -> num_keys):");
    for (count, num_keys) in update_vec {
        if (*count) > 0 {
            println!("  {} updates: {} keys", count, num_keys);
        }
    }

    let mut init_task: Vec<_> = frontier
        .drain()
        .map(|(key, value)| (key.digest(), u256_to_bytes(&value)))
        .collect();
    init_task.shuffle(&mut rand::rng());

    let (read_cnt, write_cnt, update_cnt) = access_stat
        .iter()
        .fold((0, 0, 0), |(r_acc, w_acc, u_acc), (_, (r, w, u))| {
            (r_acc + r, w_acc + w, u_acc + u)
        });
    println!("Final task {} r {} w {} u", read_cnt, write_cnt, update_cnt - write_cnt);

    fs::create_dir_all(&opts.output).unwrap();

    let mut stat_vec = access_stat.iter().collect::<Vec<_>>();
    stat_vec.sort_unstable_by_key(|(_, (x, y, z))| x + y + z);

    // println!("\nTop 1000 keys by total accesses (address, slot, reads, writes, updates):");
    // for (slot, (reads, writes, updates)) in stat_vec.iter().rev().take(1000) {
    //     println!("{:?}, {} r {} w {} u", slot, reads, writes, updates);
    // }


    // Calculate statistics for to and from for regular transactions
    let mut to_stat = HashMap::<Address, usize>::new();
    let mut from_stat = HashMap::<Address, usize>::new();
    for tx in answer.iter().flatten() {
        if tx.0.type_ == TransactionType::Regular {
            if let Some(to) = tx.0.to {
                *to_stat.entry(to).or_insert(0) += 1;
            } else {
                println!("Regular transaction with no 'to' address: {:?}", tx.0);
            }
            *from_stat.entry(tx.0.from).or_insert(0) += 1;
        }
    }
    // calculate distribution of to_stat and from_stat
    let mut to_distribution = HashMap::<usize, usize>::new();
    let mut from_distribution = HashMap::<usize, usize>::new();
    for (_, count) in &to_stat {
        *to_distribution.entry(*count).or_insert(0) += 1;
    }
    for (_, count) in &from_stat {
        *from_distribution.entry(*count).or_insert(0) += 1;
    }

    let mut to_vec: Vec<_> = to_distribution.iter().collect();
    to_vec.sort_unstable_by_key(|(count, _)| *count);
    println!("\nTo distribution (to_count -> num_addresses):");
    for (count, num_addresses) in to_vec {
        if (*count) > 0 {
            println!("  {} to: {} addresses", count, num_addresses);
        }
    }

    let mut from_vec: Vec<_> = from_distribution.iter().collect();
    from_vec.sort_unstable_by_key(|(count, _)| *count);
    println!("\nFrom distribution (from_count -> num_addresses):");
    for (count, num_addresses) in from_vec {
        if (*count) > 0 {
            println!("  {} from: {} addresses", count, num_addresses);
        }
    }

    // Calculate statistics for to and from for contract calls
    let mut to_stat_contract = HashMap::<Address, usize>::new();
    let mut from_stat_contract = HashMap::<Address, usize>::new();
    for tx in answer.iter().flatten() {
        if tx.0.type_ == TransactionType::ContractCall {
            if let Some(to) = tx.0.to {
                *to_stat_contract.entry(to).or_insert(0) += 1;
            } else {
                println!("Contract call transaction with no 'to' address: {:?}", tx.0);
            }
            *from_stat_contract.entry(tx.0.from).or_insert(0) += 1;
        }
    }
    // calculate distribution of to_stat_contract and from_stat_contract
    let mut to_distribution_contract = HashMap::<usize, usize>::new();
    let mut from_distribution_contract = HashMap::<usize, usize>::new();
    for (_, count) in &to_stat_contract {
        *to_distribution_contract.entry(*count).or_insert(0) += 1;
    }
    for (_, count) in &from_stat_contract {
        *from_distribution_contract.entry(*count).or_insert(0) += 1;
    }

    let mut to_vec_contract: Vec<_> = to_distribution_contract.iter().collect();
    to_vec_contract.sort_unstable_by_key(|(count, _)| *count);
    println!("\nTo_contract distribution (to_count -> num_addresses):");
    for (count, num_addresses) in to_vec_contract {
        if (*count) > 0 {
            println!("  {} to: {} addresses", count, num_addresses);
        }
    }

    println!("\nTop 10 contract call addresses by count:");
    let mut to_vec_contract: Vec<_> = to_stat_contract.iter().collect();
    to_vec_contract.sort_unstable_by_key(|(_, count)| *count);
    for (address, count) in to_vec_contract.iter().rev().take(10) {
        println!("  {:x}: {} calls", address, count);
    }

    let mut from_vec_contract: Vec<_> = from_distribution_contract.iter().collect();
    from_vec_contract.sort_unstable_by_key(|(count, _)| *count);
    println!("\nFrom_contract distribution (from_count -> num_addresses):");
    for (count, num_addresses) in from_vec_contract {
        if (*count) > 0 {
            println!("  {} from: {} addresses", count, num_addresses);
        }
    }

    // calculate number of db accesses per contract call transaction
    let mut contract_call_accesses: HashMap::<H160, usize> = HashMap::new();
    for tx in answer.iter().flatten() {
        if tx.0.type_ == TransactionType::ContractCall {
            *contract_call_accesses.entry(tx.0.to.unwrap_or_default()).or_insert(0) += tx.1.len();
        }
    }

    // Calculate distribution of db accesses per contract call transaction
    let mut contract_call_accesses_distribution = HashMap::<usize, usize>::new();
    for count in &contract_call_accesses {
        *contract_call_accesses_distribution.entry(*count.1).or_insert(0) += 1;
    }
    let mut contract_call_accesses_vec: Vec<_> = contract_call_accesses_distribution.iter().collect();
    contract_call_accesses_vec.sort_unstable_by_key(|(count, _)| *count);
    println!("\nAccesses_per_contract distribution (access_count -> num_contracts):");
    for (count, num_contracts) in contract_call_accesses_vec {
        if (*count) > 0 {
            println!("  {} accesses: {} contracts", count, num_contracts);
        }
    }
}

#[tokio::main]
async fn main() {
    let options: Options = Options::from_args();
    match options {
        Options::Fetch(opts) => {
            fetch_main(&opts).await;
        }
        Options::Combine(opts) => {
            combine(&opts);
        }
        Options::Seal(opts) => {
            seal(&opts);
        }
    }
}
