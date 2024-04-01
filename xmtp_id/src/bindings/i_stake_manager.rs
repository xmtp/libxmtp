pub use i_stake_manager::*;
/// This module was auto-generated with ethers-rs Abigen.
/// More information at: <https://github.com/gakonst/ethers-rs>
#[allow(
    clippy::enum_variant_names,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms,
    clippy::type_complexity,
    dead_code,
    non_camel_case_types
)]
pub mod i_stake_manager {
    pub use super::super::shared_types::*;
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("addStake"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("addStake"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("_unstakeDelaySec"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(32usize),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint32"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("balanceOf"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("balanceOf"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("account"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("depositTo"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("depositTo"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("account"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getDepositInfo"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("getDepositInfo"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("account"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("info"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                ::ethers::core::abi::ethabi::ParamType::Uint(112usize),
                                ::ethers::core::abi::ethabi::ParamType::Bool,
                                ::ethers::core::abi::ethabi::ParamType::Uint(112usize),
                                ::ethers::core::abi::ethabi::ParamType::Uint(32usize),
                                ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                            ],),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned(
                                    "struct IStakeManager.DepositInfo",
                                ),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("unlockStake"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("unlockStake"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("withdrawStake"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("withdrawStake"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("withdrawAddress"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address payable"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("withdrawTo"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("withdrawTo"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("withdrawAddress"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address payable"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("withdrawAmount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
            ]),
            events: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("Deposited"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("Deposited"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("totalDeposit"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("StakeLocked"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("StakeLocked"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("totalStaked"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: false,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("unstakeDelaySec"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("StakeUnlocked"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("StakeUnlocked"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("withdrawTime"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("StakeWithdrawn"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("StakeWithdrawn"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("withdrawAddress"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: false,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("amount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("Withdrawn"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("Withdrawn"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("withdrawAddress"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                indexed: false,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("amount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
            ]),
            errors: ::std::collections::BTreeMap::new(),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static ISTAKEMANAGER_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    pub struct IStakeManager<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for IStakeManager<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for IStakeManager<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for IStakeManager<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for IStakeManager<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(IStakeManager))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> IStakeManager<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                ISTAKEMANAGER_ABI.clone(),
                client,
            ))
        }
        ///Calls the contract's `addStake` (0x0396cb60) function
        pub fn add_stake(
            &self,
            unstake_delay_sec: u32,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([3, 150, 203, 96], unstake_delay_sec)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `balanceOf` (0x70a08231) function
        pub fn balance_of(
            &self,
            account: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([112, 160, 130, 49], account)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `depositTo` (0xb760faf9) function
        pub fn deposit_to(
            &self,
            account: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([183, 96, 250, 249], account)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getDepositInfo` (0x5287ce12) function
        pub fn get_deposit_info(
            &self,
            account: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, DepositInfo> {
            self.0
                .method_hash([82, 135, 206, 18], account)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `unlockStake` (0xbb9fe6bf) function
        pub fn unlock_stake(&self) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([187, 159, 230, 191], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `withdrawStake` (0xc23a5cea) function
        pub fn withdraw_stake(
            &self,
            withdraw_address: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([194, 58, 92, 234], withdraw_address)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `withdrawTo` (0x205c2878) function
        pub fn withdraw_to(
            &self,
            withdraw_address: ::ethers::core::types::Address,
            withdraw_amount: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([32, 92, 40, 120], (withdraw_address, withdraw_amount))
                .expect("method not found (this should never happen)")
        }
        ///Gets the contract's `Deposited` event
        pub fn deposited_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, DepositedFilter> {
            self.0.event()
        }
        ///Gets the contract's `StakeLocked` event
        pub fn stake_locked_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, StakeLockedFilter>
        {
            self.0.event()
        }
        ///Gets the contract's `StakeUnlocked` event
        pub fn stake_unlocked_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, StakeUnlockedFilter>
        {
            self.0.event()
        }
        ///Gets the contract's `StakeWithdrawn` event
        pub fn stake_withdrawn_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, StakeWithdrawnFilter>
        {
            self.0.event()
        }
        ///Gets the contract's `Withdrawn` event
        pub fn withdrawn_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, WithdrawnFilter> {
            self.0.event()
        }
        /// Returns an `Event` builder for all the events of this contract.
        pub fn events(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, IStakeManagerEvents>
        {
            self.0
                .event_with_filter(::core::default::Default::default())
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for IStakeManager<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    #[derive(
        Clone,
        ::ethers::contract::EthEvent,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethevent(name = "Deposited", abi = "Deposited(address,uint256)")]
    pub struct DepositedFilter {
        #[ethevent(indexed)]
        pub account: ::ethers::core::types::Address,
        pub total_deposit: ::ethers::core::types::U256,
    }
    #[derive(
        Clone,
        ::ethers::contract::EthEvent,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethevent(name = "StakeLocked", abi = "StakeLocked(address,uint256,uint256)")]
    pub struct StakeLockedFilter {
        #[ethevent(indexed)]
        pub account: ::ethers::core::types::Address,
        pub total_staked: ::ethers::core::types::U256,
        pub unstake_delay_sec: ::ethers::core::types::U256,
    }
    #[derive(
        Clone,
        ::ethers::contract::EthEvent,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethevent(name = "StakeUnlocked", abi = "StakeUnlocked(address,uint256)")]
    pub struct StakeUnlockedFilter {
        #[ethevent(indexed)]
        pub account: ::ethers::core::types::Address,
        pub withdraw_time: ::ethers::core::types::U256,
    }
    #[derive(
        Clone,
        ::ethers::contract::EthEvent,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethevent(
        name = "StakeWithdrawn",
        abi = "StakeWithdrawn(address,address,uint256)"
    )]
    pub struct StakeWithdrawnFilter {
        #[ethevent(indexed)]
        pub account: ::ethers::core::types::Address,
        pub withdraw_address: ::ethers::core::types::Address,
        pub amount: ::ethers::core::types::U256,
    }
    #[derive(
        Clone,
        ::ethers::contract::EthEvent,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethevent(name = "Withdrawn", abi = "Withdrawn(address,address,uint256)")]
    pub struct WithdrawnFilter {
        #[ethevent(indexed)]
        pub account: ::ethers::core::types::Address,
        pub withdraw_address: ::ethers::core::types::Address,
        pub amount: ::ethers::core::types::U256,
    }
    ///Container type for all of the contract's events
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        serde::Serialize,
        serde::Deserialize,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub enum IStakeManagerEvents {
        DepositedFilter(DepositedFilter),
        StakeLockedFilter(StakeLockedFilter),
        StakeUnlockedFilter(StakeUnlockedFilter),
        StakeWithdrawnFilter(StakeWithdrawnFilter),
        WithdrawnFilter(WithdrawnFilter),
    }
    impl ::ethers::contract::EthLogDecode for IStakeManagerEvents {
        fn decode_log(
            log: &::ethers::core::abi::RawLog,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::Error> {
            if let Ok(decoded) = DepositedFilter::decode_log(log) {
                return Ok(IStakeManagerEvents::DepositedFilter(decoded));
            }
            if let Ok(decoded) = StakeLockedFilter::decode_log(log) {
                return Ok(IStakeManagerEvents::StakeLockedFilter(decoded));
            }
            if let Ok(decoded) = StakeUnlockedFilter::decode_log(log) {
                return Ok(IStakeManagerEvents::StakeUnlockedFilter(decoded));
            }
            if let Ok(decoded) = StakeWithdrawnFilter::decode_log(log) {
                return Ok(IStakeManagerEvents::StakeWithdrawnFilter(decoded));
            }
            if let Ok(decoded) = WithdrawnFilter::decode_log(log) {
                return Ok(IStakeManagerEvents::WithdrawnFilter(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData)
        }
    }
    impl ::core::fmt::Display for IStakeManagerEvents {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::DepositedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::StakeLockedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::StakeUnlockedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::StakeWithdrawnFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::WithdrawnFilter(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<DepositedFilter> for IStakeManagerEvents {
        fn from(value: DepositedFilter) -> Self {
            Self::DepositedFilter(value)
        }
    }
    impl ::core::convert::From<StakeLockedFilter> for IStakeManagerEvents {
        fn from(value: StakeLockedFilter) -> Self {
            Self::StakeLockedFilter(value)
        }
    }
    impl ::core::convert::From<StakeUnlockedFilter> for IStakeManagerEvents {
        fn from(value: StakeUnlockedFilter) -> Self {
            Self::StakeUnlockedFilter(value)
        }
    }
    impl ::core::convert::From<StakeWithdrawnFilter> for IStakeManagerEvents {
        fn from(value: StakeWithdrawnFilter) -> Self {
            Self::StakeWithdrawnFilter(value)
        }
    }
    impl ::core::convert::From<WithdrawnFilter> for IStakeManagerEvents {
        fn from(value: WithdrawnFilter) -> Self {
            Self::WithdrawnFilter(value)
        }
    }
    ///Container type for all input parameters for the `addStake` function with signature `addStake(uint32)` and selector `0x0396cb60`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "addStake", abi = "addStake(uint32)")]
    pub struct AddStakeCall {
        pub unstake_delay_sec: u32,
    }
    ///Container type for all input parameters for the `balanceOf` function with signature `balanceOf(address)` and selector `0x70a08231`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "balanceOf", abi = "balanceOf(address)")]
    pub struct BalanceOfCall {
        pub account: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `depositTo` function with signature `depositTo(address)` and selector `0xb760faf9`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "depositTo", abi = "depositTo(address)")]
    pub struct DepositToCall {
        pub account: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `getDepositInfo` function with signature `getDepositInfo(address)` and selector `0x5287ce12`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "getDepositInfo", abi = "getDepositInfo(address)")]
    pub struct GetDepositInfoCall {
        pub account: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `unlockStake` function with signature `unlockStake()` and selector `0xbb9fe6bf`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "unlockStake", abi = "unlockStake()")]
    pub struct UnlockStakeCall;
    ///Container type for all input parameters for the `withdrawStake` function with signature `withdrawStake(address)` and selector `0xc23a5cea`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "withdrawStake", abi = "withdrawStake(address)")]
    pub struct WithdrawStakeCall {
        pub withdraw_address: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `withdrawTo` function with signature `withdrawTo(address,uint256)` and selector `0x205c2878`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "withdrawTo", abi = "withdrawTo(address,uint256)")]
    pub struct WithdrawToCall {
        pub withdraw_address: ::ethers::core::types::Address,
        pub withdraw_amount: ::ethers::core::types::U256,
    }
    ///Container type for all of the contract's call
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        serde::Serialize,
        serde::Deserialize,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub enum IStakeManagerCalls {
        AddStake(AddStakeCall),
        BalanceOf(BalanceOfCall),
        DepositTo(DepositToCall),
        GetDepositInfo(GetDepositInfoCall),
        UnlockStake(UnlockStakeCall),
        WithdrawStake(WithdrawStakeCall),
        WithdrawTo(WithdrawToCall),
    }
    impl ::ethers::core::abi::AbiDecode for IStakeManagerCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <AddStakeCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::AddStake(decoded));
            }
            if let Ok(decoded) = <BalanceOfCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::BalanceOf(decoded));
            }
            if let Ok(decoded) = <DepositToCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::DepositTo(decoded));
            }
            if let Ok(decoded) =
                <GetDepositInfoCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::GetDepositInfo(decoded));
            }
            if let Ok(decoded) = <UnlockStakeCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::UnlockStake(decoded));
            }
            if let Ok(decoded) = <WithdrawStakeCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::WithdrawStake(decoded));
            }
            if let Ok(decoded) = <WithdrawToCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::WithdrawTo(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for IStakeManagerCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AddStake(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::BalanceOf(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::DepositTo(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetDepositInfo(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::UnlockStake(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::WithdrawStake(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::WithdrawTo(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for IStakeManagerCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AddStake(element) => ::core::fmt::Display::fmt(element, f),
                Self::BalanceOf(element) => ::core::fmt::Display::fmt(element, f),
                Self::DepositTo(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetDepositInfo(element) => ::core::fmt::Display::fmt(element, f),
                Self::UnlockStake(element) => ::core::fmt::Display::fmt(element, f),
                Self::WithdrawStake(element) => ::core::fmt::Display::fmt(element, f),
                Self::WithdrawTo(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AddStakeCall> for IStakeManagerCalls {
        fn from(value: AddStakeCall) -> Self {
            Self::AddStake(value)
        }
    }
    impl ::core::convert::From<BalanceOfCall> for IStakeManagerCalls {
        fn from(value: BalanceOfCall) -> Self {
            Self::BalanceOf(value)
        }
    }
    impl ::core::convert::From<DepositToCall> for IStakeManagerCalls {
        fn from(value: DepositToCall) -> Self {
            Self::DepositTo(value)
        }
    }
    impl ::core::convert::From<GetDepositInfoCall> for IStakeManagerCalls {
        fn from(value: GetDepositInfoCall) -> Self {
            Self::GetDepositInfo(value)
        }
    }
    impl ::core::convert::From<UnlockStakeCall> for IStakeManagerCalls {
        fn from(value: UnlockStakeCall) -> Self {
            Self::UnlockStake(value)
        }
    }
    impl ::core::convert::From<WithdrawStakeCall> for IStakeManagerCalls {
        fn from(value: WithdrawStakeCall) -> Self {
            Self::WithdrawStake(value)
        }
    }
    impl ::core::convert::From<WithdrawToCall> for IStakeManagerCalls {
        fn from(value: WithdrawToCall) -> Self {
            Self::WithdrawTo(value)
        }
    }
    ///Container type for all return fields from the `balanceOf` function with signature `balanceOf(address)` and selector `0x70a08231`
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
    pub struct BalanceOfReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `getDepositInfo` function with signature `getDepositInfo(address)` and selector `0x5287ce12`
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
    pub struct GetDepositInfoReturn {
        pub info: DepositInfo,
    }
}
