pub use lib_string::*;
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
pub mod lib_string {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::std::collections::BTreeMap::new(),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("HexLengthInsufficient"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("HexLengthInsufficient",),
                        inputs: ::std::vec![],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("TooBigForSmallString"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("TooBigForSmallString",),
                        inputs: ::std::vec![],
                    },],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static LIBSTRING_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`V`7`\x0B\x82\x82\x829\x80Q`\0\x1A`s\x14`*WcNH{q`\xE0\x1B`\0R`\0`\x04R`$`\0\xFD[0`\0R`s\x81S\x82\x81\xF3\xFEs\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x14`\x80`@R`\0\x80\xFD\xFE\xA2dipfsX\"\x12 T\xF2H\x14\x86f*0e6]?\x16&\xBE{o\x9F\x86a\x0E\x1C$_\x1Cx1a\xE2\xC3b\xE7dsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static LIBSTRING_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"s\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x14`\x80`@R`\0\x80\xFD\xFE\xA2dipfsX\"\x12 T\xF2H\x14\x86f*0e6]?\x16&\xBE{o\x9F\x86a\x0E\x1C$_\x1Cx1a\xE2\xC3b\xE7dsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static LIBSTRING_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct LibString<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for LibString<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for LibString<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for LibString<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for LibString<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(LibString))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> LibString<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                LIBSTRING_ABI.clone(),
                client,
            ))
        }
        /// Constructs the general purpose `Deployer` instance based on the provided constructor arguments and sends it.
        /// Returns a new instance of a deployer that returns an instance of this contract after sending the transaction
        ///
        /// Notes:
        /// - If there are no constructor arguments, you should pass `()` as the argument.
        /// - The default poll duration is 7 seconds.
        /// - The default number of confirmations is 1 block.
        ///
        ///
        /// # Example
        ///
        /// Generate contract bindings with `abigen!` and deploy a new contract instance.
        ///
        /// *Note*: this requires a `bytecode` and `abi` object in the `greeter.json` artifact.
        ///
        /// ```ignore
        /// # async fn deploy<M: ethers::providers::Middleware>(client: ::std::sync::Arc<M>) {
        ///     abigen!(Greeter, "../greeter.json");
        ///
        ///    let greeter_contract = Greeter::deploy(client, "Hello world!".to_string()).unwrap().send().await.unwrap();
        ///    let msg = greeter_contract.greet().call().await.unwrap();
        /// # }
        /// ```
        pub fn deploy<T: ::ethers::core::abi::Tokenize>(
            client: ::std::sync::Arc<M>,
            constructor_args: T,
        ) -> ::core::result::Result<
            ::ethers::contract::builders::ContractDeployer<M, Self>,
            ::ethers::contract::ContractError<M>,
        > {
            let factory = ::ethers::contract::ContractFactory::new(
                LIBSTRING_ABI.clone(),
                LIBSTRING_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for LibString<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `HexLengthInsufficient` with signature `HexLengthInsufficient()` and selector `0x2194895a`
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
    #[etherror(name = "HexLengthInsufficient", abi = "HexLengthInsufficient()")]
    pub struct HexLengthInsufficient;
    ///Custom Error type `TooBigForSmallString` with signature `TooBigForSmallString()` and selector `0xec92f9a3`
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
    #[etherror(name = "TooBigForSmallString", abi = "TooBigForSmallString()")]
    pub struct TooBigForSmallString;
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
    pub enum LibStringErrors {
        HexLengthInsufficient(HexLengthInsufficient),
        TooBigForSmallString(TooBigForSmallString),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for LibStringErrors {
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
                <HexLengthInsufficient as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::HexLengthInsufficient(decoded));
            }
            if let Ok(decoded) =
                <TooBigForSmallString as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::TooBigForSmallString(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for LibStringErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::HexLengthInsufficient(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::TooBigForSmallString(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for LibStringErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <HexLengthInsufficient as ::ethers::contract::EthError>::selector() =>
                {
                    true
                }
                _ if selector
                    == <TooBigForSmallString as ::ethers::contract::EthError>::selector() =>
                {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for LibStringErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::HexLengthInsufficient(element) => ::core::fmt::Display::fmt(element, f),
                Self::TooBigForSmallString(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for LibStringErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<HexLengthInsufficient> for LibStringErrors {
        fn from(value: HexLengthInsufficient) -> Self {
            Self::HexLengthInsufficient(value)
        }
    }
    impl ::core::convert::From<TooBigForSmallString> for LibStringErrors {
        fn from(value: TooBigForSmallString) -> Self {
            Self::TooBigForSmallString(value)
        }
    }
}
