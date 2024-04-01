pub use erc1271_input_generator::*;
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
pub mod erc1271_input_generator {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::Some(::ethers::core::abi::ethabi::Constructor {
                inputs: ::std::vec![
                    ::ethers::core::abi::ethabi::Param {
                        name: ::std::borrow::ToOwned::to_owned("account"),
                        kind: ::ethers::core::abi::ethabi::ParamType::Address,
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("contract CoinbaseSmartWallet",),
                        ),
                    },
                    ::ethers::core::abi::ethabi::Param {
                        name: ::std::borrow::ToOwned::to_owned("hash"),
                        kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("bytes32"),
                        ),
                    },
                    ::ethers::core::abi::ethabi::Param {
                        name: ::std::borrow::ToOwned::to_owned("accountFactory"),
                        kind: ::ethers::core::abi::ethabi::ParamType::Address,
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("address"),
                        ),
                    },
                    ::ethers::core::abi::ethabi::Param {
                        name: ::std::borrow::ToOwned::to_owned("factoryCalldata"),
                        kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("bytes"),
                        ),
                    },
                ],
            }),
            functions: ::std::collections::BTreeMap::new(),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AccountDeploymentFailed"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("AccountDeploymentFailed",),
                        inputs: ::std::vec![],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("ReturnedAddressDoesNotMatchAccount"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned(
                            "ReturnedAddressDoesNotMatchAccount",
                        ),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("returned"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                        ],
                    },],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static ERC1271INPUTGENERATOR_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x10W`\0\x80\xFD[P`@Qa\x03\xAB8\x03\x80a\x03\xAB\x839\x81\x01`@\x81\x90Ra\0/\x91a\x02tV[`\0a\0=\x85\x85\x85\x85a\0IV[\x90P\x80`\x80R` `\x80\xF3[`\0`\x01`\x01`\xA0\x1B\x03\x85\x16;\x15a\0\xCBW`@Qcg\n\x83_`\xE1\x1B\x81R`\x04\x81\x01\x85\x90R`\x01`\x01`\xA0\x1B\x03\x86\x16\x90c\xCE\x15\x06\xBE\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\0\xA0W=`\0\x80>=`\0\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\0\xC4\x91\x90a\x03QV[\x90Pa\x02\x1AV[`\0\x80\x84`\x01`\x01`\xA0\x1B\x03\x16\x84`@Qa\0\xE6\x91\x90a\x03jV[`\0`@Q\x80\x83\x03\x81`\0\x86Z\xF1\x91PP=\x80`\0\x81\x14a\x01#W`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=`\0` \x84\x01>a\x01(V[``\x91P[P\x91P\x91P\x81a\x01JW`@Qb\x94UU`\xE5\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\0\x81\x80` \x01\x90Q\x81\x01\x90a\x01`\x91\x90a\x03\x86V[\x90P\x87`\x01`\x01`\xA0\x1B\x03\x16\x81`\x01`\x01`\xA0\x1B\x03\x16\x14a\x01\xABW`@Qc\xC8bC\x83`\xE0\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x80\x8A\x16`\x04\x83\x01R\x82\x16`$\x82\x01R`D\x01`@Q\x80\x91\x03\x90\xFD[`@Qcg\n\x83_`\xE1\x1B\x81R`\x04\x81\x01\x88\x90R`\x01`\x01`\xA0\x1B\x03\x89\x16\x90c\xCE\x15\x06\xBE\x90`$\x01` `@Q\x80\x83\x03\x81\x86Z\xFA\x15\x80\x15a\x01\xF0W=`\0\x80>=`\0\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x02\x14\x91\x90a\x03QV[\x93PPPP[\x94\x93PPPPV[`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x027W`\0\x80\xFD[PV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`\0[\x83\x81\x10\x15a\x02kW\x81\x81\x01Q\x83\x82\x01R` \x01a\x02SV[PP`\0\x91\x01RV[`\0\x80`\0\x80`\x80\x85\x87\x03\x12\x15a\x02\x8AW`\0\x80\xFD[\x84Qa\x02\x95\x81a\x02\"V[` \x86\x01Q`@\x87\x01Q\x91\x95P\x93Pa\x02\xAD\x81a\x02\"V[``\x86\x01Q\x90\x92P`\x01`\x01`@\x1B\x03\x80\x82\x11\x15a\x02\xCAW`\0\x80\xFD[\x81\x87\x01\x91P\x87`\x1F\x83\x01\x12a\x02\xDEW`\0\x80\xFD[\x81Q\x81\x81\x11\x15a\x02\xF0Wa\x02\xF0a\x02:V[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x83\x82\x11\x81\x83\x10\x17\x15a\x03\x18Wa\x03\x18a\x02:V[\x81`@R\x82\x81R\x8A` \x84\x87\x01\x01\x11\x15a\x031W`\0\x80\xFD[a\x03B\x83` \x83\x01` \x88\x01a\x02PV[\x97\x9A\x96\x99P\x94\x97PPPPPPV[`\0` \x82\x84\x03\x12\x15a\x03cW`\0\x80\xFD[PQ\x91\x90PV[`\0\x82Qa\x03|\x81\x84` \x87\x01a\x02PV[\x91\x90\x91\x01\x92\x91PPV[`\0` \x82\x84\x03\x12\x15a\x03\x98W`\0\x80\xFD[\x81Qa\x03\xA3\x81a\x02\"V[\x93\x92PPPV\xFE";
    /// The bytecode of the contract.
    pub static ERC1271INPUTGENERATOR_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\0\x80\xFD\xFE\xA2dipfsX\"\x12 \x86\x8Dv\xF4p\xB7z\x1B/?\x11U\xB1\xCF\x83\xC7\xDA\x8A\xA4\x93\xAF\x03\xF9\x84;\xE8\x86\xD5\xCC\xFAC\x10dsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static ERC1271INPUTGENERATOR_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct ERC1271InputGenerator<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for ERC1271InputGenerator<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for ERC1271InputGenerator<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for ERC1271InputGenerator<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for ERC1271InputGenerator<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(ERC1271InputGenerator))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> ERC1271InputGenerator<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                ERC1271INPUTGENERATOR_ABI.clone(),
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
                ERC1271INPUTGENERATOR_ABI.clone(),
                ERC1271INPUTGENERATOR_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for ERC1271InputGenerator<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `AccountDeploymentFailed` with signature `AccountDeploymentFailed()` and selector `0x128aaaa0`
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
    #[etherror(name = "AccountDeploymentFailed", abi = "AccountDeploymentFailed()")]
    pub struct AccountDeploymentFailed;
    ///Custom Error type `ReturnedAddressDoesNotMatchAccount` with signature `ReturnedAddressDoesNotMatchAccount(address,address)` and selector `0xc8624383`
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
        name = "ReturnedAddressDoesNotMatchAccount",
        abi = "ReturnedAddressDoesNotMatchAccount(address,address)"
    )]
    pub struct ReturnedAddressDoesNotMatchAccount {
        pub account: ::ethers::core::types::Address,
        pub returned: ::ethers::core::types::Address,
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
    pub enum ERC1271InputGeneratorErrors {
        AccountDeploymentFailed(AccountDeploymentFailed),
        ReturnedAddressDoesNotMatchAccount(ReturnedAddressDoesNotMatchAccount),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for ERC1271InputGeneratorErrors {
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
                <AccountDeploymentFailed as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AccountDeploymentFailed(decoded));
            }
            if let Ok(decoded) =
                <ReturnedAddressDoesNotMatchAccount as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ReturnedAddressDoesNotMatchAccount(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for ERC1271InputGeneratorErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::AccountDeploymentFailed(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::ReturnedAddressDoesNotMatchAccount(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for ERC1271InputGeneratorErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector
                    == <AccountDeploymentFailed as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector
                    == <ReturnedAddressDoesNotMatchAccount as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for ERC1271InputGeneratorErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AccountDeploymentFailed(element) => ::core::fmt::Display::fmt(element, f),
                Self::ReturnedAddressDoesNotMatchAccount(element) => {
                    ::core::fmt::Display::fmt(element, f)
                }
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for ERC1271InputGeneratorErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<AccountDeploymentFailed> for ERC1271InputGeneratorErrors {
        fn from(value: AccountDeploymentFailed) -> Self {
            Self::AccountDeploymentFailed(value)
        }
    }
    impl ::core::convert::From<ReturnedAddressDoesNotMatchAccount> for ERC1271InputGeneratorErrors {
        fn from(value: ReturnedAddressDoesNotMatchAccount) -> Self {
            Self::ReturnedAddressDoesNotMatchAccount(value)
        }
    }
}
