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
#[derive(Clone, Debug)]
pub struct Transaction {
    pub id: u64,
    pub sender: Address,
    pub recipient: Address,
    pub amount: i128,
    pub status: TransactionStatus,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Balance(Address),
    Transaction(u64),
    TxCount,
}
