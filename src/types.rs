use soroban_sdk::{contracttype, Address};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Completed,
    Escrowed,
    Released,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    pub id: u64,
    pub sender: Address,
    pub recipient: Address,
    pub amount: i128,
    pub status: TransactionStatus,
    pub timestamp: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Balance(Address),
    Transaction(u64),
    TxCount,
}
