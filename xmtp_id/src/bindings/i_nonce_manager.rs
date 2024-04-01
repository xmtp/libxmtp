pub use i_nonce_manager::*;
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
pub mod i_nonce_manager {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("getNonce"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
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
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(192usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint192"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("nonce"),
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
                    ::std::borrow::ToOwned::to_owned("incrementNonce"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("incrementNonce"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("key"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(192usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint192"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
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
    pub static INONCEMANAGER_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    pub struct INonceManager<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for INonceManager<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for INonceManager<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for INonceManager<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for INonceManager<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(INonceManager))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> INonceManager<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                INONCEMANAGER_ABI.clone(),
                client,
            ))
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
        ///Calls the contract's `incrementNonce` (0x0bd28e3b) function
        pub fn increment_nonce(
            &self,
            key: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([11, 210, 142, 59], key)
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for INonceManager<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
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
    pub enum INonceManagerCalls {
        GetNonce(GetNonceCall),
        IncrementNonce(IncrementNonceCall),
    }
    impl ::ethers::core::abi::AbiDecode for INonceManagerCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <GetNonceCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::GetNonce(decoded));
            }
            if let Ok(decoded) =
                <IncrementNonceCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IncrementNonce(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for INonceManagerCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::GetNonce(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::IncrementNonce(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for INonceManagerCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::GetNonce(element) => ::core::fmt::Display::fmt(element, f),
                Self::IncrementNonce(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<GetNonceCall> for INonceManagerCalls {
        fn from(value: GetNonceCall) -> Self {
            Self::GetNonce(value)
        }
    }
    impl ::core::convert::From<IncrementNonceCall> for INonceManagerCalls {
        fn from(value: IncrementNonceCall) -> Self {
            Self::IncrementNonce(value)
        }
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
}
