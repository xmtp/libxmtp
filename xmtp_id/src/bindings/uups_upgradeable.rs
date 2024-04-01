pub use uups_upgradeable::*;
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
pub mod uups_upgradeable {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("proxiableUUID"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("proxiableUUID"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes32"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("upgradeToAndCall"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("upgradeToAndCall"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("newImplementation"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("data"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
            ]),
            events: ::core::convert::From::from([(
                ::std::borrow::ToOwned::to_owned("Upgraded"),
                ::std::vec![::ethers::core::abi::ethabi::Event {
                    name: ::std::borrow::ToOwned::to_owned("Upgraded"),
                    inputs: ::std::vec![::ethers::core::abi::ethabi::EventParam {
                        name: ::std::borrow::ToOwned::to_owned("implementation"),
                        kind: ::ethers::core::abi::ethabi::ParamType::Address,
                        indexed: true,
                    },],
                    anonymous: false,
                },],
            )]),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("UnauthorizedCallContext"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("UnauthorizedCallContext",),
                        inputs: ::std::vec![],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("UpgradeFailed"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("UpgradeFailed"),
                        inputs: ::std::vec![],
                    },],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static UUPSUPGRADEABLE_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    pub struct UUPSUpgradeable<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for UUPSUpgradeable<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for UUPSUpgradeable<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for UUPSUpgradeable<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for UUPSUpgradeable<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(UUPSUpgradeable))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> UUPSUpgradeable<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                UUPSUPGRADEABLE_ABI.clone(),
                client,
            ))
        }
        ///Calls the contract's `proxiableUUID` (0x52d1902d) function
        pub fn proxiable_uuid(&self) -> ::ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([82, 209, 144, 45], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `upgradeToAndCall` (0x4f1ef286) function
        pub fn upgrade_to_and_call(
            &self,
            new_implementation: ::ethers::core::types::Address,
            data: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([79, 30, 242, 134], (new_implementation, data))
                .expect("method not found (this should never happen)")
        }
        ///Gets the contract's `Upgraded` event
        pub fn upgraded_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, UpgradedFilter> {
            self.0.event()
        }
        /// Returns an `Event` builder for all the events of this contract.
        pub fn events(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, UpgradedFilter> {
            self.0
                .event_with_filter(::core::default::Default::default())
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for UUPSUpgradeable<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `UnauthorizedCallContext` with signature `UnauthorizedCallContext()` and selector `0x9f03a026`
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
    #[etherror(name = "UnauthorizedCallContext", abi = "UnauthorizedCallContext()")]
    pub struct UnauthorizedCallContext;
    ///Custom Error type `UpgradeFailed` with signature `UpgradeFailed()` and selector `0x55299b49`
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
    #[etherror(name = "UpgradeFailed", abi = "UpgradeFailed()")]
    pub struct UpgradeFailed;
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
    pub enum UUPSUpgradeableErrors {
        UnauthorizedCallContext(UnauthorizedCallContext),
        UpgradeFailed(UpgradeFailed),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for UUPSUpgradeableErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) =
                <UnauthorizedCallContext as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::UnauthorizedCallContext(decoded));
            }
            if let Ok(decoded) = <UpgradeFailed as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::UpgradeFailed(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for UUPSUpgradeableErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::UnauthorizedCallContext(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UpgradeFailed(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for UUPSUpgradeableErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <UnauthorizedCallContext as ::ethers::contract::EthError>::selector() =>
                {
                    true
                }
                _ if selector == <UpgradeFailed as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for UUPSUpgradeableErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::UnauthorizedCallContext(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpgradeFailed(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for UUPSUpgradeableErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<UnauthorizedCallContext> for UUPSUpgradeableErrors {
        fn from(value: UnauthorizedCallContext) -> Self {
            Self::UnauthorizedCallContext(value)
        }
    }
    impl ::core::convert::From<UpgradeFailed> for UUPSUpgradeableErrors {
        fn from(value: UpgradeFailed) -> Self {
            Self::UpgradeFailed(value)
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
    #[ethevent(name = "Upgraded", abi = "Upgraded(address)")]
    pub struct UpgradedFilter {
        #[ethevent(indexed)]
        pub implementation: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `proxiableUUID` function with signature `proxiableUUID()` and selector `0x52d1902d`
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
    #[ethcall(name = "proxiableUUID", abi = "proxiableUUID()")]
    pub struct ProxiableUUIDCall;
    ///Container type for all input parameters for the `upgradeToAndCall` function with signature `upgradeToAndCall(address,bytes)` and selector `0x4f1ef286`
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
    #[ethcall(name = "upgradeToAndCall", abi = "upgradeToAndCall(address,bytes)")]
    pub struct UpgradeToAndCallCall {
        pub new_implementation: ::ethers::core::types::Address,
        pub data: ::ethers::core::types::Bytes,
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
    pub enum UUPSUpgradeableCalls {
        ProxiableUUID(ProxiableUUIDCall),
        UpgradeToAndCall(UpgradeToAndCallCall),
    }
    impl ::ethers::core::abi::AbiDecode for UUPSUpgradeableCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <ProxiableUUIDCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ProxiableUUID(decoded));
            }
            if let Ok(decoded) =
                <UpgradeToAndCallCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::UpgradeToAndCall(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for UUPSUpgradeableCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::ProxiableUUID(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::UpgradeToAndCall(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for UUPSUpgradeableCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::ProxiableUUID(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpgradeToAndCall(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<ProxiableUUIDCall> for UUPSUpgradeableCalls {
        fn from(value: ProxiableUUIDCall) -> Self {
            Self::ProxiableUUID(value)
        }
    }
    impl ::core::convert::From<UpgradeToAndCallCall> for UUPSUpgradeableCalls {
        fn from(value: UpgradeToAndCallCall) -> Self {
            Self::UpgradeToAndCall(value)
        }
    }
    ///Container type for all return fields from the `proxiableUUID` function with signature `proxiableUUID()` and selector `0x52d1902d`
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
    pub struct ProxiableUUIDReturn(pub [u8; 32]);
}
