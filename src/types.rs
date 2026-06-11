use soroban_sdk::{contracttype, Address};

/// Fee configuration: fee in basis points (1 bps = 0.01%) and treasury address.
/// Max fee: 10_000 bps (100%). Default: 0 bps (no fee).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeConfig {
    pub fee_bps: u32,
    pub treasury: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionStatus {
    Pending,
    Completed,
    Escrowed,
    Released,
    Cancelled,
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
    /// Ledger sequence number after which this escrow expires (0 = no expiry)
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DataKey {
    /// Admin address with contract management privileges
    Admin,
    /// User balance storage keyed by address
    Balance(Address),
    /// Transaction data storage keyed by transaction ID
    Transaction(u64),
    /// Global transaction counter for generating unique IDs
    TxCount,
    /// Configuration settings for the contract
    Config,
    /// User-specific metadata and settings
    UserMetadata(Address),
    /// Contract pause state (true if paused)
    Paused,
    /// Total supply of tokens in the system
    TotalSupply,
    /// Fee configuration and rates
    FeeConfig,
    /// Last transaction timestamp for rate limiting
    LastTxTime(Address),
}

impl DataKey {
    /// Creates a balance key for the given address
    pub fn balance(addr: Address) -> Self {
        Self::Balance(addr)
    }

    /// Creates a transaction key for the given transaction ID
    pub fn transaction(id: u64) -> Self {
        Self::Transaction(id)
    }

    /// Creates a user metadata key for the given address
    pub fn user_metadata(addr: Address) -> Self {
        Self::UserMetadata(addr)
    }

    /// Creates a last transaction time key for rate limiting
    pub fn last_tx_time(addr: Address) -> Self {
        Self::LastTxTime(addr)
    }

    /// Checks if this key is for persistent storage (long-term data)
    pub fn is_persistent(&self) -> bool {
        match self {
            Self::Balance(_)
            | Self::Transaction(_)
            | Self::UserMetadata(_)
            | Self::TotalSupply
            | Self::LastTxTime(_) => true,
            Self::Admin | Self::TxCount | Self::Config | Self::Paused | Self::FeeConfig => false,
        }
    }

    /// Checks if this key is for instance storage (contract-specific data)
    pub fn is_instance(&self) -> bool {
        !self.is_persistent()
    }

    /// Returns a human-readable description of the key
    pub fn description(&self) -> &'static str {
        match self {
            Self::Admin => "Contract administrator address",
            Self::Balance(_) => "User token balance",
            Self::Transaction(_) => "Transaction record",
            Self::TxCount => "Transaction counter",
            Self::Config => "Contract configuration",
            Self::UserMetadata(_) => "User-specific metadata",
            Self::Paused => "Contract pause state",
            Self::TotalSupply => "Total token supply",
            Self::FeeConfig => "Fee configuration settings",
            Self::LastTxTime(_) => "Last transaction timestamp",
        }
    }
}
