pub use utils::*;
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
pub mod utils {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([(
                ::std::borrow::ToOwned::to_owned("getWebAuthnStruct"),
                ::std::vec![::ethers::core::abi::ethabi::Function {
                    name: ::std::borrow::ToOwned::to_owned("getWebAuthnStruct"),
                    inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                        name: ::std::borrow::ToOwned::to_owned("challenge"),
                        kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("bytes32"),
                        ),
                    },],
                    outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                        name: ::std::string::String::new(),
                        kind: ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                            ::ethers::core::abi::ethabi::ParamType::Bytes,
                            ::ethers::core::abi::ethabi::ParamType::String,
                            ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize),
                        ],),
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("struct WebAuthnInfo"),
                        ),
                    },],
                    constant: ::core::option::Option::None,
                    state_mutability: ::ethers::core::abi::ethabi::StateMutability::Pure,
                },],
            )]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::std::collections::BTreeMap::new(),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static UTILS_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"a\x05\xEDa\0:`\x0B\x82\x82\x829\x80Q`\0\x1A`s\x14a\0-WcNH{q`\xE0\x1B`\0R`\0`\x04R`$`\0\xFD[0`\0R`s\x81S\x82\x81\xF3\xFEs\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x14`\x80`@R`\x046\x10a\x005W`\x005`\xE0\x1C\x80c\x8F\x7FYn\x14a\0:W[`\0\x80\xFD[a\0Ma\0H6`\x04a\x03%V[a\0cV[`@Qa\0Z\x91\x90a\x03\x8EV[`@Q\x80\x91\x03\x90\xF3[`@\x80Q``\x80\x82\x01\x83R\x80\x82R` \x82\x01R`\0\x91\x81\x01\x91\x90\x91R`\0a\0\xAB\x83`@Q` \x01a\0\x97\x91\x81R` \x01\x90V[`@Q` \x81\x83\x03\x03\x81R\x90`@Ra\x01\xD4V[\x90P`\0\x81`@Q` \x01a\0\xC0\x91\x90a\x03\xDEV[`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R``\x83\x01\x90\x91R`%\x80\x83R\x90\x92P`\0\x91\x90a\x05S` \x83\x019\x90P`\0`\x02\x83`@Qa\0\xFC\x91\x90a\x04}V[` `@Q\x80\x83\x03\x81\x85Z\xFA\x15\x80\x15a\x01\x19W=`\0\x80>=`\0\xFD[PPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x01<\x91\x90a\x04\x99V[\x90P`\0`\x02\x83\x83`@Q` \x01a\x01U\x92\x91\x90a\x04\xB2V[`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R\x90\x82\x90Ra\x01o\x91a\x04}V[` `@Q\x80\x83\x03\x81\x85Z\xFA\x15\x80\x15a\x01\x8CW=`\0\x80>=`\0\xFD[PPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x01\xAF\x91\x90a\x04\x99V[`@\x80Q``\x81\x01\x82R\x94\x85R` \x85\x01\x95\x90\x95R\x93\x83\x01\x93\x90\x93RP\x94\x93PPPPV[``\x81Q`\0\x03a\x01\xF3WPP`@\x80Q` \x81\x01\x90\x91R`\0\x81R\x90V[`\0`@Q\x80``\x01`@R\x80`@\x81R` \x01a\x05x`@\x919\x90P`\0`\x03\x84Q`\x02a\x02\"\x91\x90a\x04\xEAV[a\x02,\x91\x90a\x05\x03V[a\x027\x90`\x04a\x05%V[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x02OWa\x02Oa\x05<V[`@Q\x90\x80\x82R\x80`\x1F\x01`\x1F\x19\x16` \x01\x82\x01`@R\x80\x15a\x02yW` \x82\x01\x81\x806\x837\x01\x90P[P\x90P`\x01\x82\x01` \x82\x01\x85\x86Q\x87\x01[\x80\x82\x10\x15a\x02\xE5W`\x03\x82\x01\x91P\x81Q`?\x81`\x12\x1C\x16\x85\x01Q\x84S`\x01\x84\x01\x93P`?\x81`\x0C\x1C\x16\x85\x01Q\x84S`\x01\x84\x01\x93P`?\x81`\x06\x1C\x16\x85\x01Q\x84S`\x01\x84\x01\x93P`?\x81\x16\x85\x01Q\x84SP`\x01\x83\x01\x92Pa\x02\x8AV[PP`\x03\x86Q\x06`\x01\x81\x14a\x03\x01W`\x02\x81\x14a\x03\x0CWa\x03\x13V[`\x02\x82\x03\x91Pa\x03\x13V[`\x01\x82\x03\x91P[P\x82\x90\x03`\x1F\x19\x01\x82RP\x93\x92PPPV[`\0` \x82\x84\x03\x12\x15a\x037W`\0\x80\xFD[P5\x91\x90PV[`\0[\x83\x81\x10\x15a\x03YW\x81\x81\x01Q\x83\x82\x01R` \x01a\x03AV[PP`\0\x91\x01RV[`\0\x81Q\x80\x84Ra\x03z\x81` \x86\x01` \x86\x01a\x03>V[`\x1F\x01`\x1F\x19\x16\x92\x90\x92\x01` \x01\x92\x91PPV[` \x81R`\0\x82Q``` \x84\x01Ra\x03\xAA`\x80\x84\x01\x82a\x03bV[\x90P` \x84\x01Q`\x1F\x19\x84\x83\x03\x01`@\x85\x01Ra\x03\xC7\x82\x82a\x03bV[\x91PP`@\x84\x01Q``\x84\x01R\x80\x91PP\x92\x91PPV[\x7F{\"type\":\"webauthn.get\",\"challeng\x81Rc2\x91\x1D\x11`\xE1\x1B` \x82\x01R`\0\x82Qa\x04#\x81`$\x85\x01` \x87\x01a\x03>V[\x7F\",\"origin\":\"https://sign.coinbas`$\x93\x90\x91\x01\x92\x83\x01RP\x7Fe.com\",\"crossOrigin\":false}\0\0\0\0\0`D\x82\x01R`_\x01\x91\x90PV[`\0\x82Qa\x04\x8F\x81\x84` \x87\x01a\x03>V[\x91\x90\x91\x01\x92\x91PPV[`\0` \x82\x84\x03\x12\x15a\x04\xABW`\0\x80\xFD[PQ\x91\x90PV[`\0\x83Qa\x04\xC4\x81\x84` \x88\x01a\x03>V[\x91\x90\x91\x01\x91\x82RP` \x01\x91\x90PV[cNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[\x80\x82\x01\x80\x82\x11\x15a\x04\xFDWa\x04\xFDa\x04\xD4V[\x92\x91PPV[`\0\x82a\x05 WcNH{q`\xE0\x1B`\0R`\x12`\x04R`$`\0\xFD[P\x04\x90V[\x80\x82\x02\x81\x15\x82\x82\x04\x84\x14\x17a\x04\xFDWa\x04\xFDa\x04\xD4V[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD\xFEI\x96\r\xE5\x88\x0E\x8Cht4\x17\x0Fdv`[\x8F\xE4\xAE\xB9\xA2\x862\xC7\x99\\\xF3\xBA\x83\x1D\x97c\x05\0\0\0\0ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_\xA2dipfsX\"\x12 \xD2\xAD\xD6\x1E\x12\xD8p\"ML\x0E\xE2\xB8-\xCCP\xFD\xCA\x0E\xC1\x13\xCCL\xF22 L\xC0\x92\x91\xB4\x10dsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static UTILS_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"s\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x14`\x80`@R`\x046\x10a\x005W`\x005`\xE0\x1C\x80c\x8F\x7FYn\x14a\0:W[`\0\x80\xFD[a\0Ma\0H6`\x04a\x03%V[a\0cV[`@Qa\0Z\x91\x90a\x03\x8EV[`@Q\x80\x91\x03\x90\xF3[`@\x80Q``\x80\x82\x01\x83R\x80\x82R` \x82\x01R`\0\x91\x81\x01\x91\x90\x91R`\0a\0\xAB\x83`@Q` \x01a\0\x97\x91\x81R` \x01\x90V[`@Q` \x81\x83\x03\x03\x81R\x90`@Ra\x01\xD4V[\x90P`\0\x81`@Q` \x01a\0\xC0\x91\x90a\x03\xDEV[`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R``\x83\x01\x90\x91R`%\x80\x83R\x90\x92P`\0\x91\x90a\x05S` \x83\x019\x90P`\0`\x02\x83`@Qa\0\xFC\x91\x90a\x04}V[` `@Q\x80\x83\x03\x81\x85Z\xFA\x15\x80\x15a\x01\x19W=`\0\x80>=`\0\xFD[PPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x01<\x91\x90a\x04\x99V[\x90P`\0`\x02\x83\x83`@Q` \x01a\x01U\x92\x91\x90a\x04\xB2V[`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R\x90\x82\x90Ra\x01o\x91a\x04}V[` `@Q\x80\x83\x03\x81\x85Z\xFA\x15\x80\x15a\x01\x8CW=`\0\x80>=`\0\xFD[PPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x01\xAF\x91\x90a\x04\x99V[`@\x80Q``\x81\x01\x82R\x94\x85R` \x85\x01\x95\x90\x95R\x93\x83\x01\x93\x90\x93RP\x94\x93PPPPV[``\x81Q`\0\x03a\x01\xF3WPP`@\x80Q` \x81\x01\x90\x91R`\0\x81R\x90V[`\0`@Q\x80``\x01`@R\x80`@\x81R` \x01a\x05x`@\x919\x90P`\0`\x03\x84Q`\x02a\x02\"\x91\x90a\x04\xEAV[a\x02,\x91\x90a\x05\x03V[a\x027\x90`\x04a\x05%V[g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x02OWa\x02Oa\x05<V[`@Q\x90\x80\x82R\x80`\x1F\x01`\x1F\x19\x16` \x01\x82\x01`@R\x80\x15a\x02yW` \x82\x01\x81\x806\x837\x01\x90P[P\x90P`\x01\x82\x01` \x82\x01\x85\x86Q\x87\x01[\x80\x82\x10\x15a\x02\xE5W`\x03\x82\x01\x91P\x81Q`?\x81`\x12\x1C\x16\x85\x01Q\x84S`\x01\x84\x01\x93P`?\x81`\x0C\x1C\x16\x85\x01Q\x84S`\x01\x84\x01\x93P`?\x81`\x06\x1C\x16\x85\x01Q\x84S`\x01\x84\x01\x93P`?\x81\x16\x85\x01Q\x84SP`\x01\x83\x01\x92Pa\x02\x8AV[PP`\x03\x86Q\x06`\x01\x81\x14a\x03\x01W`\x02\x81\x14a\x03\x0CWa\x03\x13V[`\x02\x82\x03\x91Pa\x03\x13V[`\x01\x82\x03\x91P[P\x82\x90\x03`\x1F\x19\x01\x82RP\x93\x92PPPV[`\0` \x82\x84\x03\x12\x15a\x037W`\0\x80\xFD[P5\x91\x90PV[`\0[\x83\x81\x10\x15a\x03YW\x81\x81\x01Q\x83\x82\x01R` \x01a\x03AV[PP`\0\x91\x01RV[`\0\x81Q\x80\x84Ra\x03z\x81` \x86\x01` \x86\x01a\x03>V[`\x1F\x01`\x1F\x19\x16\x92\x90\x92\x01` \x01\x92\x91PPV[` \x81R`\0\x82Q``` \x84\x01Ra\x03\xAA`\x80\x84\x01\x82a\x03bV[\x90P` \x84\x01Q`\x1F\x19\x84\x83\x03\x01`@\x85\x01Ra\x03\xC7\x82\x82a\x03bV[\x91PP`@\x84\x01Q``\x84\x01R\x80\x91PP\x92\x91PPV[\x7F{\"type\":\"webauthn.get\",\"challeng\x81Rc2\x91\x1D\x11`\xE1\x1B` \x82\x01R`\0\x82Qa\x04#\x81`$\x85\x01` \x87\x01a\x03>V[\x7F\",\"origin\":\"https://sign.coinbas`$\x93\x90\x91\x01\x92\x83\x01RP\x7Fe.com\",\"crossOrigin\":false}\0\0\0\0\0`D\x82\x01R`_\x01\x91\x90PV[`\0\x82Qa\x04\x8F\x81\x84` \x87\x01a\x03>V[\x91\x90\x91\x01\x92\x91PPV[`\0` \x82\x84\x03\x12\x15a\x04\xABW`\0\x80\xFD[PQ\x91\x90PV[`\0\x83Qa\x04\xC4\x81\x84` \x88\x01a\x03>V[\x91\x90\x91\x01\x91\x82RP` \x01\x91\x90PV[cNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[\x80\x82\x01\x80\x82\x11\x15a\x04\xFDWa\x04\xFDa\x04\xD4V[\x92\x91PPV[`\0\x82a\x05 WcNH{q`\xE0\x1B`\0R`\x12`\x04R`$`\0\xFD[P\x04\x90V[\x80\x82\x02\x81\x15\x82\x82\x04\x84\x14\x17a\x04\xFDWa\x04\xFDa\x04\xD4V[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD\xFEI\x96\r\xE5\x88\x0E\x8Cht4\x17\x0Fdv`[\x8F\xE4\xAE\xB9\xA2\x862\xC7\x99\\\xF3\xBA\x83\x1D\x97c\x05\0\0\0\0ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_\xA2dipfsX\"\x12 \xD2\xAD\xD6\x1E\x12\xD8p\"ML\x0E\xE2\xB8-\xCCP\xFD\xCA\x0E\xC1\x13\xCCL\xF22 L\xC0\x92\x91\xB4\x10dsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static UTILS_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct Utils<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for Utils<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for Utils<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for Utils<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for Utils<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(Utils))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> Utils<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                UTILS_ABI.clone(),
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
                UTILS_ABI.clone(),
                UTILS_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `getWebAuthnStruct` (0x8f7f596e) function
        pub fn get_web_authn_struct(
            &self,
            challenge: [u8; 32],
        ) -> ::ethers::contract::builders::ContractCall<M, WebAuthnInfo> {
            self.0
                .method_hash([143, 127, 89, 110], challenge)
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for Utils<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Container type for all input parameters for the `getWebAuthnStruct` function with signature `getWebAuthnStruct(bytes32)` and selector `0x8f7f596e`
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
    #[ethcall(name = "getWebAuthnStruct", abi = "getWebAuthnStruct(bytes32)")]
    pub struct GetWebAuthnStructCall {
        pub challenge: [u8; 32],
    }
    ///Container type for all return fields from the `getWebAuthnStruct` function with signature `getWebAuthnStruct(bytes32)` and selector `0x8f7f596e`
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
    pub struct GetWebAuthnStructReturn(pub WebAuthnInfo);
    ///`WebAuthnInfo(bytes,string,bytes32)`
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
    pub struct WebAuthnInfo {
        pub authenticator_data: ::ethers::core::types::Bytes,
        pub client_data_json: ::std::string::String,
        pub message_hash: [u8; 32],
    }
}
