pub use i_aggregator::*;
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
pub mod i_aggregator {
    pub use super::super::shared_types::*;
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("aggregateSignatures"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("aggregateSignatures",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("userOps"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                ::std::boxed::Box::new(
                                    ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
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
                                    ],),
                                ),
                            ),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct UserOperation[]"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("aggregatedSignature",),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("validateSignatures"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("validateSignatures"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("userOps"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
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
                                        ],),
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("struct UserOperation[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("signature"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("validateUserOpSignature"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("validateUserOpSignature",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("userOp"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
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
                            ],),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("struct UserOperation"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("sigForUserOp"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::std::collections::BTreeMap::new(),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static IAGGREGATOR_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    pub struct IAggregator<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for IAggregator<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for IAggregator<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for IAggregator<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for IAggregator<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(IAggregator))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> IAggregator<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                IAGGREGATOR_ABI.clone(),
                client,
            ))
        }
        ///Calls the contract's `aggregateSignatures` (0x275e2d79) function
        pub fn aggregate_signatures(
            &self,
            user_ops: ::std::vec::Vec<UserOperation>,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Bytes> {
            self.0
                .method_hash([39, 94, 45, 121], user_ops)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `validateSignatures` (0xe3563a4f) function
        pub fn validate_signatures(
            &self,
            user_ops: ::std::vec::Vec<UserOperation>,
            signature: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([227, 86, 58, 79], (user_ops, signature))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `validateUserOpSignature` (0x64c530cd) function
        pub fn validate_user_op_signature(
            &self,
            user_op: UserOperation,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Bytes> {
            self.0
                .method_hash([100, 197, 48, 205], (user_op,))
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for IAggregator<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Container type for all input parameters for the `aggregateSignatures` function with signature `aggregateSignatures((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[])` and selector `0x275e2d79`
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
        name = "aggregateSignatures",
        abi = "aggregateSignatures((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[])"
    )]
    pub struct AggregateSignaturesCall {
        pub user_ops: ::std::vec::Vec<UserOperation>,
    }
    ///Container type for all input parameters for the `validateSignatures` function with signature `validateSignatures((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],bytes)` and selector `0xe3563a4f`
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
        name = "validateSignatures",
        abi = "validateSignatures((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[],bytes)"
    )]
    pub struct ValidateSignaturesCall {
        pub user_ops: ::std::vec::Vec<UserOperation>,
        pub signature: ::ethers::core::types::Bytes,
    }
    ///Container type for all input parameters for the `validateUserOpSignature` function with signature `validateUserOpSignature((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))` and selector `0x64c530cd`
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
        name = "validateUserOpSignature",
        abi = "validateUserOpSignature((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))"
    )]
    pub struct ValidateUserOpSignatureCall {
        pub user_op: UserOperation,
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
    pub enum IAggregatorCalls {
        AggregateSignatures(AggregateSignaturesCall),
        ValidateSignatures(ValidateSignaturesCall),
        ValidateUserOpSignature(ValidateUserOpSignatureCall),
    }
    impl ::ethers::core::abi::AbiDecode for IAggregatorCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <AggregateSignaturesCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AggregateSignatures(decoded));
            }
            if let Ok(decoded) =
                <ValidateSignaturesCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ValidateSignatures(decoded));
            }
            if let Ok(decoded) =
                <ValidateUserOpSignatureCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ValidateUserOpSignature(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for IAggregatorCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AggregateSignatures(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ValidateSignatures(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ValidateUserOpSignature(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
            }
        }
    }
    impl ::core::fmt::Display for IAggregatorCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AggregateSignatures(element) => ::core::fmt::Display::fmt(element, f),
                Self::ValidateSignatures(element) => ::core::fmt::Display::fmt(element, f),
                Self::ValidateUserOpSignature(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AggregateSignaturesCall> for IAggregatorCalls {
        fn from(value: AggregateSignaturesCall) -> Self {
            Self::AggregateSignatures(value)
        }
    }
    impl ::core::convert::From<ValidateSignaturesCall> for IAggregatorCalls {
        fn from(value: ValidateSignaturesCall) -> Self {
            Self::ValidateSignatures(value)
        }
    }
    impl ::core::convert::From<ValidateUserOpSignatureCall> for IAggregatorCalls {
        fn from(value: ValidateUserOpSignatureCall) -> Self {
            Self::ValidateUserOpSignature(value)
        }
    }
    ///Container type for all return fields from the `aggregateSignatures` function with signature `aggregateSignatures((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes)[])` and selector `0x275e2d79`
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
    pub struct AggregateSignaturesReturn {
        pub aggregated_signature: ::ethers::core::types::Bytes,
    }
    ///Container type for all return fields from the `validateUserOpSignature` function with signature `validateUserOpSignature((address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes))` and selector `0x64c530cd`
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
    pub struct ValidateUserOpSignatureReturn {
        pub sig_for_user_op: ::ethers::core::types::Bytes,
    }
}
