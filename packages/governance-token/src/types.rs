use soroban_sdk::{contracterror, contractevent, contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct AllowanceData {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    NotInitialized = 1,
    Unauthorized = 2,
    InsufficientBalance = 3,
    InvalidAmount = 4,
    Overflow = 5,
    AllowanceExpired = 6,
    AllowanceExceeded = 7,
    NegativeAllowance = 8,
    NotAdmin = 9,
    ContractPaused = 10,
    Underflow = 11,
    Frozen = 12,
    RecipientNotAllowlisted = 13,
    CannotTransferToSelf = 16,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct Transfer {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct Approve {
    pub owner: Address,
    pub spender: Address,
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct Mint {
    pub to: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct Burn {
    pub from: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct Clawback {
    pub from: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AccountFrozen {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AccountUnfrozen {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AllowlistModeChanged {
    pub enabled: bool,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AllowlistAdded {
    pub account: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AllowlistRemoved {
    pub account: Address,
}
