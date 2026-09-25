use std::collections::HashSet;

use ethers::types::{H160, U256};
use ordermap::OrderSet;
use postcard::from_bytes;
use rand::seq::SliceRandom;

use crate::{BlockAccess, DBAccess, PageKey, SlotKey, opts::AnalyzeOptions, read_from_file};

pub fn analyze_trace(opts: &AnalyzeOptions, is_naive: bool, contract_shuffle_mode: i32) {
    let loaded = std::fs::read(&opts.input).unwrap();
    let answer: Vec<BlockAccess> = from_bytes(&loaded).unwrap();

    let mut new_addresses: OrderSet<H160> = OrderSet::new();
    let mut last_file_accounts: Vec<H160> = read_from_file("./data/25000000_700000.accounts");
    if is_naive {
        last_file_accounts.shuffle(&mut rand::rng());
    }
    new_addresses.extend(last_file_accounts);

    let batch_size = 100000;
    let values_per_page = 32;
    let mut batch_number = 0;
    let mut page_buffer = HashSet::<PageKey>::new();
    let mut account_page_buffer_hits = 0;
    let mut contract_page_buffer_hits = 0;
    let account_cache_size = 65536;
    let contract_cache_size = 1;
    let mut contract_storage_indexes = OrderSet::<H160>::new();
    let mut contract_storage_record_numbers = Vec::<OrderSet::<SlotKey>>::new();
    if is_naive {
        contract_storage_indexes.insert(H160::zero());
        contract_storage_record_numbers.push(OrderSet::<SlotKey>::new());
    }
    let mut account_cache: Vec<H160> = vec![H160::zero(); account_cache_size];
    let mut contract_cache: Vec<SlotKey> = vec![SlotKey::default(); contract_cache_size];
    let mut account_cache_hits = 0;
    let mut contract_cache_hits = 0;
    let mut done_transactions = 0;
    let mut block_accesses: Vec<DBAccess> = Vec::new();
    for tx_list in answer.iter() {
        for tx in tx_list.iter() {
            batch_number += 1;
            handle_account_access(&new_addresses, values_per_page, &mut page_buffer, &mut account_page_buffer_hits, account_cache_size, &mut account_cache, &mut account_cache_hits, tx.0.from);
            if tx.0.to.is_some() {
                handle_account_access(&new_addresses, values_per_page, &mut page_buffer, &mut account_page_buffer_hits, account_cache_size, &mut account_cache, &mut account_cache_hits, tx.0.to.unwrap());
            }
            let mut db_accesses = tx.1.clone();
            if contract_shuffle_mode == 1 {
                db_accesses.shuffle(&mut rand::rng());
            }
            if contract_shuffle_mode == 2 {
                block_accesses.extend(db_accesses);
            } else {
                for access in &tx.1 {
                    match access {
                        DBAccess::Read(slot, _) | DBAccess::Write(slot, _) => {
                            let mut contract_storage_index: usize;
                            if is_naive {
                                contract_storage_index = 0;
                            } else {
                                contract_storage_index = contract_storage_indexes.get_index_of(&slot.address).unwrap_or(usize::MAX);
                                if contract_storage_index == usize::MAX {
                                    contract_storage_indexes.insert(slot.address);
                                    contract_storage_index = contract_storage_indexes.len() - 1;
                                    contract_storage_record_numbers.push(OrderSet::<SlotKey>::new());
                                }
                            }
                            let contract_record_numbers = &mut contract_storage_record_numbers[contract_storage_index];
                            let mut slot_record_number = contract_record_numbers.get_index_of(slot).unwrap_or(usize::MAX);
                            if slot_record_number == usize::MAX {
                                slot_record_number = contract_record_numbers.len();
                                contract_record_numbers.insert(slot.clone());
                            }
                            let cache_key = (slot.address.to_low_u64_le() ^ slot.slot.low_u64()) % (contract_cache_size as u64);
                            if contract_cache[cache_key as usize] == slot.clone() {
                                contract_cache_hits += 1;
                            } else {
                                contract_cache[cache_key as usize] = slot.clone();
                                let slot_page_number = (slot_record_number / values_per_page) as i32;
                                let slot_page_key: PageKey;
                                if is_naive {
                                    slot_page_key = PageKey { index: slot_page_number, address: H160::zero() };
                                } else {
                                    slot_page_key = PageKey { index: slot_page_number, address: slot.address };
                                }
                                if page_buffer.contains(&slot_page_key) {
                                    contract_page_buffer_hits += 1;
                                }
                                page_buffer.insert(slot_page_key);
                            }
                        }
                    }
                    batch_number += 1;
                }
            }
            if batch_number >= batch_size {
                batch_number = 0;
                page_buffer.clear();
            }
            done_transactions += 1;
            if done_transactions % 100000 == 0 {
                println!("Processed {} transactions", done_transactions);
            }
        }
    }
    if contract_shuffle_mode == 2 {
        block_accesses.shuffle(&mut rand::rng());
        for access in &block_accesses {
            match access {
                DBAccess::Read(slot, _) | DBAccess::Write(slot, _) => {
                    let mut contract_storage_index: usize;
                    if is_naive {
                        contract_storage_index = 0;
                    } else {
                        contract_storage_index = contract_storage_indexes.get_index_of(&slot.address).unwrap_or(usize::MAX);
                        if contract_storage_index == usize::MAX {
                            contract_storage_indexes.insert(slot.address);
                            contract_storage_index = contract_storage_indexes.len() - 1;
                            contract_storage_record_numbers.push(OrderSet::<SlotKey>::new());
                        }
                    }
                    let contract_record_numbers = &mut contract_storage_record_numbers[contract_storage_index];
                    let mut slot_record_number = contract_record_numbers.get_index_of(slot).unwrap_or(usize::MAX);
                    if slot_record_number == usize::MAX {
                        slot_record_number = contract_record_numbers.len();
                        contract_record_numbers.insert(slot.clone());
                    }
                    let cache_key = (slot.address.to_low_u64_le() ^ slot.slot.low_u64()) % (contract_cache_size as u64);
                    if contract_cache[cache_key as usize] == slot.clone() {
                        contract_cache_hits += 1;
                    } else {
                        contract_cache[cache_key as usize] = slot.clone();
                        let slot_page_number = (slot_record_number / values_per_page) as i32;
                        let slot_page_key: PageKey;
                        if is_naive {
                            slot_page_key = PageKey { index: slot_page_number, address: H160::zero() };
                        } else {
                            slot_page_key = PageKey { index: slot_page_number, address: slot.address };
                        }
                        if page_buffer.contains(&slot_page_key) {
                            contract_page_buffer_hits += 1;
                        }
                        page_buffer.insert(slot_page_key);
                    }
                }
            }
            batch_number += 1;
            if batch_number >= batch_size {
                batch_number = 0;
                page_buffer.clear();
            }
        }
        block_accesses.clear();
    }
    println!("Contract shuffle mode: {}", contract_shuffle_mode);
    println!("Total transactions: {}", answer.iter().flatten().count());
    println!("Total accesses: {}", answer.iter().flatten().map(|tx| tx.1.len()).sum::<usize>());
    println!("Page buffer hits: {}", account_page_buffer_hits);
    println!("Cache hits: {}", account_cache_hits);
    println!("Contract page buffer hits: {}", contract_page_buffer_hits);
    println!("Contract cache hits: {}", contract_cache_hits);
    println!("Total unique accounts: {}", new_addresses.len());
    if is_naive {
        println!("Total unique contract storage slots: {}", contract_storage_record_numbers[0].len());
    } else {
        let total_contract_storage_slots: usize = contract_storage_record_numbers.iter().map(|s| s.len()).sum();
        println!("Total unique contract storage slots: {}", total_contract_storage_slots);
    }
}

fn handle_account_access(new_addresses: &OrderSet<H160>, values_per_page: usize, page_buffer: &mut HashSet<PageKey>, account_page_buffer_hits: &mut i32, account_cache_size: usize, account_cache: &mut Vec<H160>, account_cache_hits: &mut i32, address: H160) {
    let record_number = new_addresses.get_index_of(&address).unwrap_or(usize::MAX);
    let account_storage_hash = H160::from_low_u64_be(8758937604987);
    if record_number == usize::MAX {
        println!("From address not found in new_addresses: {:?}", address);
    }
    let page_number = (record_number / values_per_page) as i32;
    if account_cache[record_number % account_cache_size] == address {
        *account_cache_hits += 1;
    } else {
        account_cache[record_number % account_cache_size] = address;
        let page_key = PageKey { index: page_number, address: account_storage_hash };
        if page_buffer.contains(&page_key) {
            *account_page_buffer_hits += 1;
        }
        page_buffer.insert(page_key);
    }
}