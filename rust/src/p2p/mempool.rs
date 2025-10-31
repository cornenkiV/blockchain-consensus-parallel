use crate::pos::Transaction;
use std::collections::HashSet;

/// tx pool for pending transactions
pub struct TransactionPool {
    pending: Vec<Transaction>,
    capacity: usize,
    seen_signatures: HashSet<String>, // for duplicate transactions
}

impl TransactionPool {
    pub fn new(capacity: usize) -> Self {
        TransactionPool {
            pending: Vec::new(),
            capacity,
            seen_signatures: HashSet::new(),
        }
    }

    pub fn add_transaction(&mut self, tx: Transaction) -> Result<(), String> {
        if self.pending.len() >= self.capacity {
            return Err("Transaction pool is full".to_string());
        }

        if self.seen_signatures.contains(&tx.signature) {
            return Err("Duplicate transaction".to_string());
        }

        self.seen_signatures.insert(tx.signature.clone());
        self.pending.push(tx);
        Ok(())
    }

    pub fn get_transactions(&self, count: usize) -> Vec<Transaction> {
        self.pending.iter().take(count).cloned().collect()
    }

    pub fn remove_transactions(&mut self, txs: &[Transaction]) {
        let signatures_to_remove: HashSet<_> = txs.iter().map(|tx| tx.signature.clone()).collect();

        self.pending
            .retain(|tx| !signatures_to_remove.contains(&tx.signature));

        for sig in signatures_to_remove {
            self.seen_signatures.remove(&sig);
        }
    }

    pub fn size(&self) -> usize {
        self.pending.len()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.seen_signatures.clear();
    }

    pub fn get_all(&self) -> Vec<Transaction> {
        self.pending.clone()
    }
}
