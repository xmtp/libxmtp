pub use lib_clone::*;
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
pub mod lib_clone {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::std::collections::BTreeMap::new(),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("DeploymentFailed"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("DeploymentFailed"),
                        inputs: ::std::vec![],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("ETHTransferFailed"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("ETHTransferFailed"),
                        inputs: ::std::vec![],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("SaltDoesNotStartWith"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("SaltDoesNotStartWith",),
                        inputs: ::std::vec![],
                    },],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static LIBCLONE_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`V`7`\x0B\x82\x82\x829\x80Q`\0\x1A`s\x14`*WcNH{q`\xE0\x1B`\0R`\0`\x04R`$`\0\xFD[0`\0R`s\x81S\x82\x81\xF3\xFEs\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x14`\x80`@R`\0\x80\xFD\xFE\xA2dipfsX\"\x12 +\xDE\xA0\xC2\x9C\x15\xB7e\x0E\xEC\xDA\xEA1=\xE6\x98\x87\xD5\x10\xF2\xB5$\x03Q\xEB\xF7\t\xB7B0\xB5ldsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static LIBCLONE_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"s\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x14`\x80`@R`\0\x80\xFD\xFE\xA2dipfsX\"\x12 +\xDE\xA0\xC2\x9C\x15\xB7e\x0E\xEC\xDA\xEA1=\xE6\x98\x87\xD5\x10\xF2\xB5$\x03Q\xEB\xF7\t\xB7B0\xB5ldsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static LIBCLONE_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct LibClone<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for LibClone<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for LibClone<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for LibClone<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for LibClone<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(LibClone))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> LibClone<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                LIBCLONE_ABI.clone(),
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
                LIBCLONE_ABI.clone(),
                LIBCLONE_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for LibClone<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `DeploymentFailed` with signature `DeploymentFailed()` and selector `0x30116425`
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
    #[etherror(name = "DeploymentFailed", abi = "DeploymentFailed()")]
    pub struct DeploymentFailed;
    ///Custom Error type `ETHTransferFailed` with signature `ETHTransferFailed()` and selector `0xb12d13eb`
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
    #[etherror(name = "ETHTransferFailed", abi = "ETHTransferFailed()")]
    pub struct ETHTransferFailed;
    ///Custom Error type `SaltDoesNotStartWith` with signature `SaltDoesNotStartWith()` and selector `0x0c4549ef`
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
    #[etherror(name = "SaltDoesNotStartWith", abi = "SaltDoesNotStartWith()")]
    pub struct SaltDoesNotStartWith;
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
    pub enum LibCloneErrors {
        DeploymentFailed(DeploymentFailed),
        ETHTransferFailed(ETHTransferFailed),
        SaltDoesNotStartWith(SaltDoesNotStartWith),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for LibCloneErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) = <DeploymentFailed as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::DeploymentFailed(decoded));
            }
            if let Ok(decoded) = <ETHTransferFailed as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ETHTransferFailed(decoded));
            }
            if let Ok(decoded) =
                <SaltDoesNotStartWith as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::SaltDoesNotStartWith(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for LibCloneErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::DeploymentFailed(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ETHTransferFailed(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::SaltDoesNotStartWith(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for LibCloneErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector == <DeploymentFailed as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <ETHTransferFailed as ::ethers::contract::EthError>::selector() =>
                {
                    true
                }
                _ if selector
                    == <SaltDoesNotStartWith as ::ethers::contract::EthError>::selector() =>
                {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for LibCloneErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::DeploymentFailed(element) => ::core::fmt::Display::fmt(element, f),
                Self::ETHTransferFailed(element) => ::core::fmt::Display::fmt(element, f),
                Self::SaltDoesNotStartWith(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for LibCloneErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<DeploymentFailed> for LibCloneErrors {
        fn from(value: DeploymentFailed) -> Self {
            Self::DeploymentFailed(value)
        }
    }
    impl ::core::convert::From<ETHTransferFailed> for LibCloneErrors {
        fn from(value: ETHTransferFailed) -> Self {
            Self::ETHTransferFailed(value)
        }
    }
    impl ::core::convert::From<SaltDoesNotStartWith> for LibCloneErrors {
        fn from(value: SaltDoesNotStartWith) -> Self {
            Self::SaltDoesNotStartWith(value)
        }
    }
}
