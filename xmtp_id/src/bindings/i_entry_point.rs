pub use i_entry_point::*;
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
pub mod i_entry_point {
    pub use super::super::shared_types::*;
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("addStake"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("addStake"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("_unstakeDelaySec"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(32usize),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint32"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("balanceOf"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("balanceOf"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("account"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("depositTo"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("depositTo"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("account"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getDepositInfo"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("getDepositInfo"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("account"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("info"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(112usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bool,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(112usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(32usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.DepositInfo",
                                        ),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getNonce"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("getNonce"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("sender"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("key"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        192usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint192"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("nonce"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getSenderAddress"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("getSenderAddress"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("initCode"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getUserOpHash"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("getUserOpHash"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("userOp"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Address,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("struct UserOperation"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::string::String::new(),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        32usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes32"),
                                    ),
                                },
                            ],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("handleAggregatedOps"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned(
                                "handleAggregatedOps",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("opsPerAggregator"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Array(
                                                        ::std::boxed::Box::new(
                                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                                ::std::vec![
                                                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                                ],
                                                            ),
                                                        ),
                                                    ),
                                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IEntryPoint.UserOpsPerAggregator[]",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("beneficiary"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address payable"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("handleOps"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("handleOps"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("ops"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                        ::std::boxed::Box::new(
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                                ],
                                            ),
                                        ),
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("struct UserOperation[]"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("beneficiary"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address payable"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("incrementNonce"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("incrementNonce"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("key"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        192usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint192"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("simulateHandleOp"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("simulateHandleOp"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("op"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Address,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("struct UserOperation"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("target"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("targetCallData"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("simulateValidation"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("simulateValidation"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("userOp"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Address,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("struct UserOperation"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("unlockStake"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("unlockStake"),
                            inputs: ::std::vec![],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("withdrawStake"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
                            name: ::std::borrow::ToOwned::to_owned("withdrawStake"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("withdrawAddress"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address payable"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("withdrawTo"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Function {
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
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                            ],
                            outputs: ::std::vec![],
                            constant: ::core::option::Option::None,
                            state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                        },
                    ],
                ),
            ]),
            events: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AccountDeployed"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("AccountDeployed"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("userOpHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        32usize,
                                    ),
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("sender"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("factory"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("paymaster"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("BeforeExecution"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("BeforeExecution"),
                            inputs: ::std::vec![],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("Deposited"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("Deposited"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("account"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("totalDeposit"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("SignatureAggregatorChanged"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned(
                                "SignatureAggregatorChanged",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("aggregator"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("StakeLocked"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("StakeLocked"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("account"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("totalStaked"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("unstakeDelaySec"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("StakeUnlocked"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("StakeUnlocked"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("account"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("withdrawTime"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("StakeWithdrawn"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
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
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("UserOperationEvent"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned("UserOperationEvent"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("userOpHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        32usize,
                                    ),
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("sender"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("paymaster"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("nonce"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("success"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("actualGasCost"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("actualGasUsed"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("UserOperationRevertReason"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
                            name: ::std::borrow::ToOwned::to_owned(
                                "UserOperationRevertReason",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("userOpHash"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(
                                        32usize,
                                    ),
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("sender"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    indexed: true,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("nonce"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                                ::ethers::core::abi::ethabi::EventParam {
                                    name: ::std::borrow::ToOwned::to_owned("revertReason"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("Withdrawn"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::Event {
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
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    indexed: false,
                                },
                            ],
                            anonymous: false,
                        },
                    ],
                ),
            ]),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("ExecutionResult"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("ExecutionResult"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("preOpGas"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("paid"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("validAfter"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint48"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("validUntil"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint48"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("targetSuccess"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bool"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("targetResult"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("bytes"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("FailedOp"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("FailedOp"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("opIndex"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Uint(
                                        256usize,
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("uint256"),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("reason"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::String,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("string"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("SenderAddressResult"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "SenderAddressResult",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("sender"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("SignatureValidationFailed"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "SignatureValidationFailed",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("aggregator"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned("address"),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("ValidationResult"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned("ValidationResult"),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("returnInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bool,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IEntryPoint.ReturnInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("senderInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.StakeInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("factoryInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.StakeInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("paymasterInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.StakeInfo",
                                        ),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("ValidationResultWithAggregation"),
                    ::std::vec![
                        ::ethers::core::abi::ethabi::AbiError {
                            name: ::std::borrow::ToOwned::to_owned(
                                "ValidationResultWithAggregation",
                            ),
                            inputs: ::std::vec![
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("returnInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bool,
                                            ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(48usize),
                                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IEntryPoint.ReturnInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("senderInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.StakeInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("factoryInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.StakeInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("paymasterInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                            ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IStakeManager.StakeInfo",
                                        ),
                                    ),
                                },
                                ::ethers::core::abi::ethabi::Param {
                                    name: ::std::borrow::ToOwned::to_owned("aggregatorInfo"),
                                    kind: ::ethers::core::abi::ethabi::ParamType::Tuple(
                                        ::std::vec![
                                            ::ethers::core::abi::ethabi::ParamType::Address,
                                            ::ethers::core::abi::ethabi::ParamType::Tuple(
                                                ::std::vec![
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                                ],
                                            ),
                                        ],
                                    ),
                                    internal_type: ::core::option::Option::Some(
                                        ::std::borrow::ToOwned::to_owned(
                                            "struct IEntryPoint.AggregatorStakeInfo",
                                        ),
                                    ),
                                },
                            ],
                        },
                    ],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static IENTRYPOINT_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    pub struct IEntryPoint<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for IEntryPoint<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for IEntryPoint<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for IEntryPoint<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for IEntryPoint<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(IEntryPoint))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> IEntryPoint<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                IENTRYPOINT_ABI.clone(),
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
        ///Calls the contract's `getNonce` (0x35567e1a) function
        pub fn get_nonce(
            &self,
            sender: ::ethers::core::types::Address,
            key: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([53, 86, 126, 26], (sender, key))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getSenderAddress` (0x9b249f69) function
        pub fn get_sender_address(
            &self,
            init_code: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([155, 36, 159, 105], init_code)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getUserOpHash` (0xa6193531) function
        pub fn get_user_op_hash(
            &self,
            user_op: UserOperation,
        ) -> ::ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([166, 25, 53, 49], (user_op,))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `handleAggregatedOps` (0x4b1d7cf5) function
        pub fn handle_aggregated_ops(
            &self,
            ops_per_aggregator: ::std::vec::Vec<UserOpsPerAggregator>,
            beneficiary: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([75, 29, 124, 245], (ops_per_aggregator, beneficiary))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `handleOps` (0x1fad948c) function
        pub fn handle_ops(
            &self,
            ops: ::std::vec::Vec<UserOperation>,
            beneficiary: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([31, 173, 148, 140], (ops, beneficiary))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `incrementNonce` (0x0bd28e3b) function
        pub fn increment_nonce(
            &self,
            key: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([11, 210, 142, 59], key)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `simulateHandleOp` (0xd6383f94) function
        pub fn simulate_handle_op(
            &self,
            op: UserOperation,
            target: ::ethers::core::types::Address,
            target_call_data: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([214, 56, 63, 148], (op, target, target_call_data))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `simulateValidation` (0xee219423) function
        pub fn simulate_validation(
            &self,
            user_op: UserOperation,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([238, 33, 148, 35], (user_op,))
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
        ///Gets the contract's `AccountDeployed` event
        pub fn account_deployed_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, AccountDeployedFilter>
        {
            self.0.event()
        }
        ///Gets the contract's `BeforeExecution` event
        pub fn before_execution_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, BeforeExecutionFilter>
        {
            self.0.event()
        }
        ///Gets the contract's `Deposited` event
        pub fn deposited_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, DepositedFilter> {
            self.0.event()
        }
        ///Gets the contract's `SignatureAggregatorChanged` event
        pub fn signature_aggregator_changed_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<
            ::std::sync::Arc<M>,
            M,
            SignatureAggregatorChangedFilter,
        > {
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
        ///Gets the contract's `UserOperationEvent` event
        pub fn user_operation_event_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, UserOperationEventFilter>
        {
            self.0.event()
        }
        ///Gets the contract's `UserOperationRevertReason` event
        pub fn user_operation_revert_reason_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<
            ::std::sync::Arc<M>,
            M,
            UserOperationRevertReasonFilter,
        > {
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
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, IEntryPointEvents>
        {
            self.0
                .event_with_filter(::core::default::Default::default())
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for IEntryPoint<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `ExecutionResult` with signature `ExecutionResult(uint256,uint256,uint48,uint48,bool,bytes)` and selector `0x8b7ac980`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "ExecutionResult",
        abi = "ExecutionResult(uint256,uint256,uint48,uint48,bool,bytes)"
    )]
    pub struct ExecutionResult {
        pub pre_op_gas: ::ethers::core::types::U256,
        pub paid: ::ethers::core::types::U256,
        pub valid_after: u64,
        pub valid_until: u64,
        pub target_success: bool,
        pub target_result: ::ethers::core::types::Bytes,
    }
    ///Custom Error type `FailedOp` with signature `FailedOp(uint256,string)` and selector `0x220266b6`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(name = "FailedOp", abi = "FailedOp(uint256,string)")]
    pub struct FailedOp {
        pub op_index: ::ethers::core::types::U256,
        pub reason: ::std::string::String,
    }
    ///Custom Error type `SenderAddressResult` with signature `SenderAddressResult(address)` and selector `0x6ca7b806`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(name = "SenderAddressResult", abi = "SenderAddressResult(address)")]
    pub struct SenderAddressResult {
        pub sender: ::ethers::core::types::Address,
    }
    ///Custom Error type `SignatureValidationFailed` with signature `SignatureValidationFailed(address)` and selector `0x86a9f750`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "SignatureValidationFailed",
        abi = "SignatureValidationFailed(address)"
    )]
    pub struct SignatureValidationFailed {
        pub aggregator: ::ethers::core::types::Address,
    }
    ///Custom Error type `ValidationResult` with signature `ValidationResult((uint256,uint256,bool,uint48,uint48,bytes),(uint256,uint256),(uint256,uint256),(uint256,uint256))` and selector `0xe0cff05f`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "ValidationResult",
        abi = "ValidationResult((uint256,uint256,bool,uint48,uint48,bytes),(uint256,uint256),(uint256,uint256),(uint256,uint256))"
    )]
    pub struct ValidationResult {
        pub return_info: (
            ::ethers::core::types::U256,
            ::ethers::core::types::U256,
            bool,
            u64,
            u64,
            ::ethers::core::types::Bytes,
        ),
        pub sender_info: (::ethers::core::types::U256, ::ethers::core::types::U256),
        pub factory_info: (::ethers::core::types::U256, ::ethers::core::types::U256),
        pub paymaster_info: (::ethers::core::types::U256, ::ethers::core::types::U256),
    }
    ///Custom Error type `ValidationResultWithAggregation` with signature `ValidationResultWithAggregation((uint256,uint256,bool,uint48,uint48,bytes),(uint256,uint256),(uint256,uint256),(uint256,uint256),(address,(uint256,uint256)))` and selector `0xfaecb4e4`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        serde::Serialize,
        serde::Deserialize,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(
        name = "ValidationResultWithAggregation",
        abi = "ValidationResultWithAggregation((uint256,uint256,bool,uint48,uint48,bytes),(uint256,uint256),(uint256,uint256),(uint256,uint256),(address,(uint256,uint256)))"
    )]
    pub struct ValidationResultWithAggregation {
        pub return_info: (
            ::ethers::core::types::U256,
            ::ethers::core::types::U256,
            bool,
            u64,
            u64,
            ::ethers::core::types::Bytes,
        ),
        pub sender_info: (::ethers::core::types::U256, ::ethers::core::types::U256),
        pub factory_info: (::ethers::core::types::U256, ::ethers::core::types::U256),
        pub paymaster_info: (::ethers::core::types::U256, ::ethers::core::types::U256),
        pub aggregator_info: (
            ::ethers::core::types::Address,
            (::ethers::core::types::U256, ::ethers::core::types::U256),
        ),
    }
    ///Container type for all of the contract's custom errors
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
    pub enum IEntryPointErrors {
        ExecutionResult(ExecutionResult),
        FailedOp(FailedOp),
        SenderAddressResult(SenderAddressResult),
        SignatureValidationFailed(SignatureValidationFailed),
        ValidationResult(ValidationResult),
        ValidationResultWithAggregation(ValidationResultWithAggregation),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for IEntryPointErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) = <ExecutionResult as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::ExecutionResult(decoded));
            }
            if let Ok(decoded) = <FailedOp as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::FailedOp(decoded));
            }
            if let Ok(decoded) =
                <SenderAddressResult as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SenderAddressResult(decoded));
            }
            if let Ok(decoded) =
                <SignatureValidationFailed as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SignatureValidationFailed(decoded));
            }
            if let Ok(decoded) = <ValidationResult as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ValidationResult(decoded));
            }
            if let Ok(decoded) =
                <ValidationResultWithAggregation as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ValidationResultWithAggregation(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for IEntryPointErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::ExecutionResult(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::FailedOp(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::SenderAddressResult(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SignatureValidationFailed(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ValidationResult(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ValidationResultWithAggregation(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for IEntryPointErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <ExecutionResult as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <FailedOp as ::ethers::contract::EthError>::selector() => true,
                _ if selector
                    == <SenderAddressResult as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <SignatureValidationFailed as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <ValidationResult as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <ValidationResultWithAggregation as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for IEntryPointErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::ExecutionResult(element) => ::core::fmt::Display::fmt(element, f),
                Self::FailedOp(element) => ::core::fmt::Display::fmt(element, f),
                Self::SenderAddressResult(element) => ::core::fmt::Display::fmt(element, f),
                Self::SignatureValidationFailed(element) => ::core::fmt::Display::fmt(element, f),
                Self::ValidationResult(element) => ::core::fmt::Display::fmt(element, f),
                Self::ValidationResultWithAggregation(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for IEntryPointErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<ExecutionResult> for IEntryPointErrors {
        fn from(value: ExecutionResult) -> Self {
            Self::ExecutionResult(value)
        }
    }
    impl ::core::convert::From<FailedOp> for IEntryPointErrors {
        fn from(value: FailedOp) -> Self {
            Self::FailedOp(value)
        }
    }
    impl ::core::convert::From<SenderAddressResult> for IEntryPointErrors {
        fn from(value: SenderAddressResult) -> Self {
            Self::SenderAddressResult(value)
        }
    }
    impl ::core::convert::From<SignatureValidationFailed> for IEntryPointErrors {
        fn from(value: SignatureValidationFailed) -> Self {
            Self::SignatureValidationFailed(value)
        }
    }
    impl ::core::convert::From<ValidationResult> for IEntryPointErrors {
        fn from(value: ValidationResult) -> Self {
            Self::ValidationResult(value)
        }
    }
    impl ::core::convert::From<ValidationResultWithAggregation> for IEntryPointErrors {
        fn from(value: ValidationResultWithAggregation) -> Self {
            Self::ValidationResultWithAggregation(value)
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
    #[ethevent(
        name = "AccountDeployed",
        abi = "AccountDeployed(bytes32,address,address,address)"
    )]
    pub struct AccountDeployedFilter {
        #[ethevent(indexed)]
        pub user_op_hash: [u8; 32],
        #[ethevent(indexed)]
        pub sender: ::ethers::core::types::Address,
        pub factory: ::ethers::core::types::Address,
        pub paymaster: ::ethers::core::types::Address,
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
    #[ethevent(name = "BeforeExecution", abi = "BeforeExecution()")]
    pub struct BeforeExecutionFilter;
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
    #[ethevent(
        name = "SignatureAggregatorChanged",
        abi = "SignatureAggregatorChanged(address)"
    )]
    pub struct SignatureAggregatorChangedFilter {
        #[ethevent(indexed)]
        pub aggregator: ::ethers::core::types::Address,
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
    #[ethevent(
        name = "UserOperationEvent",
        abi = "UserOperationEvent(bytes32,address,address,uint256,bool,uint256,uint256)"
    )]
    pub struct UserOperationEventFilter {
        #[ethevent(indexed)]
        pub user_op_hash: [u8; 32],
        #[ethevent(indexed)]
        pub sender: ::ethers::core::types::Address,
        #[ethevent(indexed)]
        pub paymaster: ::ethers::core::types::Address,
        pub nonce: ::ethers::core::types::U256,
        pub success: bool,
        pub actual_gas_cost: ::ethers::core::types::U256,
        pub actual_gas_used: ::ethers::core::types::U256,
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
        name = "UserOperationRevertReason",
        abi = "UserOperationRevertReason(bytes32,address,uint256,bytes)"
    )]
    pub struct UserOperationRevertReasonFilter {
        #[ethevent(indexed)]
        pub user_op_hash: [u8; 32],
        #[ethevent(indexed)]
        pub sender: ::ethers::core::types::Address,
        pub nonce: ::ethers::core::types::U256,
        pub revert_reason: ::ethers::core::types::Bytes,
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
    pub enum IEntryPointEvents {
        AccountDeployedFilter(AccountDeployedFilter),
        BeforeExecutionFilter(BeforeExecutionFilter),
        DepositedFilter(DepositedFilter),
        SignatureAggregatorChangedFilter(SignatureAggregatorChangedFilter),
        StakeLockedFilter(StakeLockedFilter),
        StakeUnlockedFilter(StakeUnlockedFilter),
        StakeWithdrawnFilter(StakeWithdrawnFilter),
        UserOperationEventFilter(UserOperationEventFilter),
        UserOperationRevertReasonFilter(UserOperationRevertReasonFilter),
        WithdrawnFilter(WithdrawnFilter),
    }
    impl ::ethers::contract::EthLogDecode for IEntryPointEvents {
        fn decode_log(
            log: &::ethers::core::abi::RawLog,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::Error> {
            if let Ok(decoded) = AccountDeployedFilter::decode_log(log) {
                return Ok(IEntryPointEvents::AccountDeployedFilter(decoded));
            }
            if let Ok(decoded) = BeforeExecutionFilter::decode_log(log) {
                return Ok(IEntryPointEvents::BeforeExecutionFilter(decoded));
            }
            if let Ok(decoded) = DepositedFilter::decode_log(log) {
                return Ok(IEntryPointEvents::DepositedFilter(decoded));
            }
            if let Ok(decoded) = SignatureAggregatorChangedFilter::decode_log(log) {
                return Ok(IEntryPointEvents::SignatureAggregatorChangedFilter(decoded));
            }
            if let Ok(decoded) = StakeLockedFilter::decode_log(log) {
                return Ok(IEntryPointEvents::StakeLockedFilter(decoded));
            }
            if let Ok(decoded) = StakeUnlockedFilter::decode_log(log) {
                return Ok(IEntryPointEvents::StakeUnlockedFilter(decoded));
            }
            if let Ok(decoded) = StakeWithdrawnFilter::decode_log(log) {
                return Ok(IEntryPointEvents::StakeWithdrawnFilter(decoded));
            }
            if let Ok(decoded) = UserOperationEventFilter::decode_log(log) {
                return Ok(IEntryPointEvents::UserOperationEventFilter(decoded));
            }
            if let Ok(decoded) = UserOperationRevertReasonFilter::decode_log(log) {
                return Ok(IEntryPointEvents::UserOperationRevertReasonFilter(decoded));
            }
            if let Ok(decoded) = WithdrawnFilter::decode_log(log) {
                return Ok(IEntryPointEvents::WithdrawnFilter(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData)
        }
    }
    impl ::core::fmt::Display for IEntryPointEvents {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountDeployedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::BeforeExecutionFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::DepositedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::SignatureAggregatorChangedFilter(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::StakeLockedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::StakeUnlockedFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::StakeWithdrawnFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::UserOperationEventFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::UserOperationRevertReasonFilter(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::WithdrawnFilter(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AccountDeployedFilter> for IEntryPointEvents {
        fn from(value: AccountDeployedFilter) -> Self {
            Self::AccountDeployedFilter(value)
        }
    }
    impl ::core::convert::From<BeforeExecutionFilter> for IEntryPointEvents {
        fn from(value: BeforeExecutionFilter) -> Self {
            Self::BeforeExecutionFilter(value)
        }
    }
    impl ::core::convert::From<DepositedFilter> for IEntryPointEvents {
        fn from(value: DepositedFilter) -> Self {
            Self::DepositedFilter(value)
        }
    }
    impl ::core::convert::From<SignatureAggregatorChangedFilter> for IEntryPointEvents {
        fn from(value: SignatureAggregatorChangedFilter) -> Self {
            Self::SignatureAggregatorChangedFilter(value)
        }
    }
    impl ::core::convert::From<StakeLockedFilter> for IEntryPointEvents {
        fn from(value: StakeLockedFilter) -> Self {
            Self::StakeLockedFilter(value)
        }
    }
    impl ::core::convert::From<StakeUnlockedFilter> for IEntryPointEvents {
        fn from(value: StakeUnlockedFilter) -> Self {
            Self::StakeUnlockedFilter(value)
        }
    }
    impl ::core::convert::From<StakeWithdrawnFilter> for IEntryPointEvents {
        fn from(value: StakeWithdrawnFilter) -> Self {
            Self::StakeWithdrawnFilter(value)
        }
    }
    impl ::core::convert::From<UserOperationEventFilter> for IEntryPointEvents {
        fn from(value: UserOperationEventFilter) -> Self {
            Self::UserOperationEventFilter(value)
        }
    }
    impl ::core::convert::From<UserOperationRevertReasonFilter> for IEntryPointEvents {
        fn from(value: UserOperationRevertReasonFilter) -> Self {
            Self::UserOperationRevertReasonFilter(value)
        }
    }
    impl ::core::convert::From<WithdrawnFilter> for IEntryPointEvents {
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
    ///Container type for all input parameters for the `getNonce` function with signature `getNonce(address,uint192)` and selector `0x35567e1a`
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
    #[ethcall(name = "getNonce", abi = "getNonce(address,uint192)")]
    pub struct GetNonceCall {
        pub sender: ::ethers::core::types::Address,
        pub key: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `getSenderAddress` function with signature `getSenderAddress(bytes)` and selector `0x9b249f69`
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
    #[ethcall(name = "getSenderAddress", abi = "getSenderAddress(bytes)")]
    pub struct GetSenderAddressCall {
        pub init_code: ::ethers::core::types::Bytes,
    }
    ///Container type for all input parameters for the `getUserOpHash` function with signature `getUserOpHash((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))` and selector `0xa6193531`
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
    #[ethcall(
        name = "getUserOpHash",
        abi = "getUserOpHash((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))"
    )]
    pub struct GetUserOpHashCall {
        pub user_op: UserOperation,
    }
    ///Container type for all input parameters for the `handleAggregatedOps` function with signature `handleAggregatedOps(((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],address,bytes)[],address)` and selector `0x4b1d7cf5`
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
    #[ethcall(
        name = "handleAggregatedOps",
        abi = "handleAggregatedOps(((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],address,bytes)[],address)"
    )]
    pub struct HandleAggregatedOpsCall {
        pub ops_per_aggregator: ::std::vec::Vec<UserOpsPerAggregator>,
        pub beneficiary: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `handleOps` function with signature `handleOps((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],address)` and selector `0x1fad948c`
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
    #[ethcall(
        name = "handleOps",
        abi = "handleOps((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],address)"
    )]
    pub struct HandleOpsCall {
        pub ops: ::std::vec::Vec<UserOperation>,
        pub beneficiary: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `incrementNonce` function with signature `incrementNonce(uint192)` and selector `0x0bd28e3b`
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
    #[ethcall(name = "incrementNonce", abi = "incrementNonce(uint192)")]
    pub struct IncrementNonceCall {
        pub key: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `simulateHandleOp` function with signature `simulateHandleOp((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes),address,bytes)` and selector `0xd6383f94`
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
    #[ethcall(
        name = "simulateHandleOp",
        abi = "simulateHandleOp((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes),address,bytes)"
    )]
    pub struct SimulateHandleOpCall {
        pub op: UserOperation,
        pub target: ::ethers::core::types::Address,
        pub target_call_data: ::ethers::core::types::Bytes,
    }
    ///Container type for all input parameters for the `simulateValidation` function with signature `simulateValidation((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))` and selector `0xee219423`
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
    #[ethcall(
        name = "simulateValidation",
        abi = "simulateValidation((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))"
    )]
    pub struct SimulateValidationCall {
        pub user_op: UserOperation,
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
    pub enum IEntryPointCalls {
        AddStake(AddStakeCall),
        BalanceOf(BalanceOfCall),
        DepositTo(DepositToCall),
        GetDepositInfo(GetDepositInfoCall),
        GetNonce(GetNonceCall),
        GetSenderAddress(GetSenderAddressCall),
        GetUserOpHash(GetUserOpHashCall),
        HandleAggregatedOps(HandleAggregatedOpsCall),
        HandleOps(HandleOpsCall),
        IncrementNonce(IncrementNonceCall),
        SimulateHandleOp(SimulateHandleOpCall),
        SimulateValidation(SimulateValidationCall),
        UnlockStake(UnlockStakeCall),
        WithdrawStake(WithdrawStakeCall),
        WithdrawTo(WithdrawToCall),
    }
    impl ::ethers::core::abi::AbiDecode for IEntryPointCalls {
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
            if let Ok(decoded) = <GetNonceCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::GetNonce(decoded));
            }
            if let Ok(decoded) =
                <GetSenderAddressCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::GetSenderAddress(decoded));
            }
            if let Ok(decoded) = <GetUserOpHashCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::GetUserOpHash(decoded));
            }
            if let Ok(decoded) =
                <HandleAggregatedOpsCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::HandleAggregatedOps(decoded));
            }
            if let Ok(decoded) = <HandleOpsCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::HandleOps(decoded));
            }
            if let Ok(decoded) =
                <IncrementNonceCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IncrementNonce(decoded));
            }
            if let Ok(decoded) =
                <SimulateHandleOpCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SimulateHandleOp(decoded));
            }
            if let Ok(decoded) =
                <SimulateValidationCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SimulateValidation(decoded));
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
    impl ::ethers::core::abi::AbiEncode for IEntryPointCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AddStake(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::BalanceOf(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::DepositTo(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetDepositInfo(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetNonce(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetSenderAddress(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetUserOpHash(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::HandleAggregatedOps(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::HandleOps(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::IncrementNonce(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::SimulateHandleOp(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::SimulateValidation(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UnlockStake(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::WithdrawStake(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::WithdrawTo(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for IEntryPointCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AddStake(element) => ::core::fmt::Display::fmt(element, f),
                Self::BalanceOf(element) => ::core::fmt::Display::fmt(element, f),
                Self::DepositTo(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetDepositInfo(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetNonce(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetSenderAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetUserOpHash(element) => ::core::fmt::Display::fmt(element, f),
                Self::HandleAggregatedOps(element) => ::core::fmt::Display::fmt(element, f),
                Self::HandleOps(element) => ::core::fmt::Display::fmt(element, f),
                Self::IncrementNonce(element) => ::core::fmt::Display::fmt(element, f),
                Self::SimulateHandleOp(element) => ::core::fmt::Display::fmt(element, f),
                Self::SimulateValidation(element) => ::core::fmt::Display::fmt(element, f),
                Self::UnlockStake(element) => ::core::fmt::Display::fmt(element, f),
                Self::WithdrawStake(element) => ::core::fmt::Display::fmt(element, f),
                Self::WithdrawTo(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AddStakeCall> for IEntryPointCalls {
        fn from(value: AddStakeCall) -> Self {
            Self::AddStake(value)
        }
    }
    impl ::core::convert::From<BalanceOfCall> for IEntryPointCalls {
        fn from(value: BalanceOfCall) -> Self {
            Self::BalanceOf(value)
        }
    }
    impl ::core::convert::From<DepositToCall> for IEntryPointCalls {
        fn from(value: DepositToCall) -> Self {
            Self::DepositTo(value)
        }
    }
    impl ::core::convert::From<GetDepositInfoCall> for IEntryPointCalls {
        fn from(value: GetDepositInfoCall) -> Self {
            Self::GetDepositInfo(value)
        }
    }
    impl ::core::convert::From<GetNonceCall> for IEntryPointCalls {
        fn from(value: GetNonceCall) -> Self {
            Self::GetNonce(value)
        }
    }
    impl ::core::convert::From<GetSenderAddressCall> for IEntryPointCalls {
        fn from(value: GetSenderAddressCall) -> Self {
            Self::GetSenderAddress(value)
        }
    }
    impl ::core::convert::From<GetUserOpHashCall> for IEntryPointCalls {
        fn from(value: GetUserOpHashCall) -> Self {
            Self::GetUserOpHash(value)
        }
    }
    impl ::core::convert::From<HandleAggregatedOpsCall> for IEntryPointCalls {
        fn from(value: HandleAggregatedOpsCall) -> Self {
            Self::HandleAggregatedOps(value)
        }
    }
    impl ::core::convert::From<HandleOpsCall> for IEntryPointCalls {
        fn from(value: HandleOpsCall) -> Self {
            Self::HandleOps(value)
        }
    }
    impl ::core::convert::From<IncrementNonceCall> for IEntryPointCalls {
        fn from(value: IncrementNonceCall) -> Self {
            Self::IncrementNonce(value)
        }
    }
    impl ::core::convert::From<SimulateHandleOpCall> for IEntryPointCalls {
        fn from(value: SimulateHandleOpCall) -> Self {
            Self::SimulateHandleOp(value)
        }
    }
    impl ::core::convert::From<SimulateValidationCall> for IEntryPointCalls {
        fn from(value: SimulateValidationCall) -> Self {
            Self::SimulateValidation(value)
        }
    }
    impl ::core::convert::From<UnlockStakeCall> for IEntryPointCalls {
        fn from(value: UnlockStakeCall) -> Self {
            Self::UnlockStake(value)
        }
    }
    impl ::core::convert::From<WithdrawStakeCall> for IEntryPointCalls {
        fn from(value: WithdrawStakeCall) -> Self {
            Self::WithdrawStake(value)
        }
    }
    impl ::core::convert::From<WithdrawToCall> for IEntryPointCalls {
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
    ///Container type for all return fields from the `getNonce` function with signature `getNonce(address,uint192)` and selector `0x35567e1a`
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
    pub struct GetNonceReturn {
        pub nonce: ::ethers::core::types::U256,
    }
    ///Container type for all return fields from the `getUserOpHash` function with signature `getUserOpHash((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))` and selector `0xa6193531`
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
    pub struct GetUserOpHashReturn(pub [u8; 32]);
    ///`UserOpsPerAggregator((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],address,bytes)`
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
    pub struct UserOpsPerAggregator {
        pub user_ops: ::std::vec::Vec<UserOperation>,
        pub aggregator: ::ethers::core::types::Address,
        pub signature: ::ethers::core::types::Bytes,
    }
}
