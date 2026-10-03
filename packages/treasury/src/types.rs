use soroban_sdk::{contracterror, contracttype, Address};
#[contracttype]
#[derive(Clone, Debug)]
pub struct Deposit {
    pub from: Address,
    pub amount: i128,
    pub circle_id: Address,
    pub timestamp: u64,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct Withdrawal {
    pub admin: Address,
    pub to: Address,
    pub amount: i128,
    pub timestamp: u64,
}
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Balance,
    Deposits,
    Withdrawals,
    Token,
}
#[contracterror]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreasuryError {
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InsufficientBalance = 4,
    InvalidAmount = 5,
    AlreadyInitialized = 6,
    ContractNotPaused = 7,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeeDeposited {
    pub from: Address,
    pub amount: i128,
    pub circle_id: Address,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct FundsWithdrawn {
    pub to: Address,
    pub amount: i128,
}
#[contracttype]
#[derive(Clone, Debug)]
pub struct TokensRescued {
    pub to: Address,
    pub token: Address,
    pub amount: i128,
}
