///`Call(address,uint256,bytes)`
#[derive(
    Clone,
    ::ethers::contract::EthAbiType,
    ::ethers::contract::EthAbiCodec,
    serde::Serialize,
    serde::Deserialize,
    Default,
    Debug,
    PartialEq,
    Eq,
    Hash,
)]
pub struct Call {
    pub target: ::ethers::core::types::Address,
    pub value: ::ethers::core::types::U256,
    pub data: ::ethers::core::types::Bytes,
}
///`DepositInfo(uint112,bool,uint112,uint32,uint48)`
#[derive(
    Clone,
    ::ethers::contract::EthAbiType,
    ::ethers::contract::EthAbiCodec,
    serde::Serialize,
    serde::Deserialize,
    Default,
    Debug,
    PartialEq,
    Eq,
    Hash,
)]
pub struct DepositInfo {
    pub deposit: u128,
    pub staked: bool,
    pub stake: u128,
    pub unstake_delay_sec: u32,
    pub withdraw_time: u64,
}
///`UserOperation(address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)`
#[derive(
    Clone,
    ::ethers::contract::EthAbiType,
    ::ethers::contract::EthAbiCodec,
    serde::Serialize,
    serde::Deserialize,
    Default,
    Debug,
    PartialEq,
    Eq,
    Hash,
)]
pub struct UserOperation {
    pub sender: ::ethers::core::types::Address,
    pub nonce: ::ethers::core::types::U256,
    pub init_code: ::ethers::core::types::Bytes,
    pub call_data: ::ethers::core::types::Bytes,
    pub call_gas_limit: ::ethers::core::types::U256,
    pub verification_gas_limit: ::ethers::core::types::U256,
    pub pre_verification_gas: ::ethers::core::types::U256,
    pub max_fee_per_gas: ::ethers::core::types::U256,
    pub max_priority_fee_per_gas: ::ethers::core::types::U256,
    pub paymaster_and_data: ::ethers::core::types::Bytes,
    pub signature: ::ethers::core::types::Bytes,
}
