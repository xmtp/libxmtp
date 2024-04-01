pub use dummy::*;
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
pub mod dummy {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("dummy"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("dummy"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
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
            events: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("Done"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("Done"),
                        inputs: ::std::vec![],
                        anonymous: false,
                    },],
                ),
                (
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
                ),
            ]),
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
    pub static DUMMY_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\xA0`@R0`\x80R4\x80\x15a\0\x14W`\0\x80\xFD[P`\x80Qa\x02\xB6a\x006`\09`\0\x81\x81`\xB7\x01Ra\x01\x94\x01Ra\x02\xB6`\0\xF3\xFE`\x80`@R`\x046\x10a\x004W`\x005`\xE0\x1C\x80c2\xE4:\x11\x14a\09W\x80cO\x1E\xF2\x86\x14a\0PW\x80cR\xD1\x90-\x14a\0cW[`\0\x80\xFD[4\x80\x15a\0EW`\0\x80\xFD[Pa\0Na\0\x8AV[\0[a\0Na\0^6`\x04a\x01\xEFV[a\0\xB5V[4\x80\x15a\0oW`\0\x80\xFD[Pa\0xa\x01\x90V[`@Q\x90\x81R` \x01`@Q\x80\x91\x03\x90\xF3[`@Q\x7F\x9F\x9F\xB44WGI\xB7DX\xE0\xDD\xC3\xCF_\xD5\xBD\xB1\xB0\t\xC8a^\x82V\x06\xB57$Wo5\x90`\0\x90\xA1V[\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x81\x03a\0\xEBWc\x9F\x03\xA0&`\0R`\x04`\x1C\xFD[\x83``\x1B``\x1C\x93PcR\xD1\x90-`\x01R\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v\xCC75\xA9 \xA3\xCAP]8+\xBC\x80` `\x01`\x04`\x1D\x89Z\xFAQ\x14a\x01=WcU)\x9BI`\x01R`\x04`\x1D\xFD[\x84\x7F\xBC|\xD7Z \xEE'\xFD\x9A\xDE\xBA\xB3 A\xF7U!M\xBCk\xFF\xA9\x0C\xC0\"[9\xDA.\\-;`\08\xA2\x84\x90U\x81\x15a\x01\x8AW`@Q\x82\x84\x827`\08\x84\x83\x88Z\xF4a\x01\x88W=`\0\x82>=\x81\xFD[P[PPPPV[`\0\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x81\x14a\x01\xC8Wc\x9F\x03\xA0&`\0R`\x04`\x1C\xFD[\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v\xCC75\xA9 \xA3\xCAP]8+\xBC\x91PP\x90V[`\0\x80`\0`@\x84\x86\x03\x12\x15a\x02\x04W`\0\x80\xFD[\x835`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x02\x1BW`\0\x80\xFD[\x92P` \x84\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x028W`\0\x80\xFD[\x81\x86\x01\x91P\x86`\x1F\x83\x01\x12a\x02LW`\0\x80\xFD[\x815\x81\x81\x11\x15a\x02[W`\0\x80\xFD[\x87` \x82\x85\x01\x01\x11\x15a\x02mW`\0\x80\xFD[` \x83\x01\x94P\x80\x93PPPP\x92P\x92P\x92V\xFE\xA2dipfsX\"\x12 \xC0_GN:KSv\n\n\x1Fs\x98xg\xACN\xD4\x98\xFDh\x9E\x93\x88h?\x84r\xA9{i/dsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static DUMMY_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10a\x004W`\x005`\xE0\x1C\x80c2\xE4:\x11\x14a\09W\x80cO\x1E\xF2\x86\x14a\0PW\x80cR\xD1\x90-\x14a\0cW[`\0\x80\xFD[4\x80\x15a\0EW`\0\x80\xFD[Pa\0Na\0\x8AV[\0[a\0Na\0^6`\x04a\x01\xEFV[a\0\xB5V[4\x80\x15a\0oW`\0\x80\xFD[Pa\0xa\x01\x90V[`@Q\x90\x81R` \x01`@Q\x80\x91\x03\x90\xF3[`@Q\x7F\x9F\x9F\xB44WGI\xB7DX\xE0\xDD\xC3\xCF_\xD5\xBD\xB1\xB0\t\xC8a^\x82V\x06\xB57$Wo5\x90`\0\x90\xA1V[\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x81\x03a\0\xEBWc\x9F\x03\xA0&`\0R`\x04`\x1C\xFD[\x83``\x1B``\x1C\x93PcR\xD1\x90-`\x01R\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v\xCC75\xA9 \xA3\xCAP]8+\xBC\x80` `\x01`\x04`\x1D\x89Z\xFAQ\x14a\x01=WcU)\x9BI`\x01R`\x04`\x1D\xFD[\x84\x7F\xBC|\xD7Z \xEE'\xFD\x9A\xDE\xBA\xB3 A\xF7U!M\xBCk\xFF\xA9\x0C\xC0\"[9\xDA.\\-;`\08\xA2\x84\x90U\x81\x15a\x01\x8AW`@Q\x82\x84\x827`\08\x84\x83\x88Z\xF4a\x01\x88W=`\0\x82>=\x81\xFD[P[PPPPV[`\0\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x000\x81\x14a\x01\xC8Wc\x9F\x03\xA0&`\0R`\x04`\x1C\xFD[\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v\xCC75\xA9 \xA3\xCAP]8+\xBC\x91PP\x90V[`\0\x80`\0`@\x84\x86\x03\x12\x15a\x02\x04W`\0\x80\xFD[\x835`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x02\x1BW`\0\x80\xFD[\x92P` \x84\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x028W`\0\x80\xFD[\x81\x86\x01\x91P\x86`\x1F\x83\x01\x12a\x02LW`\0\x80\xFD[\x815\x81\x81\x11\x15a\x02[W`\0\x80\xFD[\x87` \x82\x85\x01\x01\x11\x15a\x02mW`\0\x80\xFD[` \x83\x01\x94P\x80\x93PPPP\x92P\x92P\x92V\xFE\xA2dipfsX\"\x12 \xC0_GN:KSv\n\n\x1Fs\x98xg\xACN\xD4\x98\xFDh\x9E\x93\x88h?\x84r\xA9{i/dsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static DUMMY_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct Dummy<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for Dummy<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for Dummy<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for Dummy<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for Dummy<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(Dummy))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> Dummy<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                DUMMY_ABI.clone(),
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
                DUMMY_ABI.clone(),
                DUMMY_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `dummy` (0x32e43a11) function
        pub fn dummy(&self) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([50, 228, 58, 17], ())
                .expect("method not found (this should never happen)")
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
        ///Gets the contract's `Done` event
        pub fn done_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, DoneFilter> {
            self.0.event()
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
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, DummyEvents> {
            self.0
                .event_with_filter(::core::default::Default::default())
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for Dummy<M> {
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
    pub enum DummyErrors {
        UnauthorizedCallContext(UnauthorizedCallContext),
        UpgradeFailed(UpgradeFailed),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for DummyErrors {
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
    impl ::ethers::core::abi::AbiEncode for DummyErrors {
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
    impl ::ethers::contract::ContractRevert for DummyErrors {
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
    impl ::core::fmt::Display for DummyErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::UnauthorizedCallContext(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpgradeFailed(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for DummyErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<UnauthorizedCallContext> for DummyErrors {
        fn from(value: UnauthorizedCallContext) -> Self {
            Self::UnauthorizedCallContext(value)
        }
    }
    impl ::core::convert::From<UpgradeFailed> for DummyErrors {
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
    #[ethevent(name = "Done", abi = "Done()")]
    pub struct DoneFilter;
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
    pub enum DummyEvents {
        DoneFilter(DoneFilter),
        UpgradedFilter(UpgradedFilter),
    }
    impl ::ethers::contract::EthLogDecode for DummyEvents {
        fn decode_log(
            log: &::ethers::core::abi::RawLog,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::Error> {
            if let Ok(decoded) = DoneFilter::decode_log(log) {
                return Ok(DummyEvents::DoneFilter(decoded));
            }
            if let Ok(decoded) = UpgradedFilter::decode_log(log) {
                return Ok(DummyEvents::UpgradedFilter(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData)
        }
    }
    impl ::core::fmt::Display for DummyEvents {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::DoneFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpgradedFilter(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<DoneFilter> for DummyEvents {
        fn from(value: DoneFilter) -> Self {
            Self::DoneFilter(value)
        }
    }
    impl ::core::convert::From<UpgradedFilter> for DummyEvents {
        fn from(value: UpgradedFilter) -> Self {
            Self::UpgradedFilter(value)
        }
    }
    ///Container type for all input parameters for the `dummy` function with signature `dummy()` and selector `0x32e43a11`
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
    #[ethcall(name = "dummy", abi = "dummy()")]
    pub struct DummyCall;
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
    pub enum DummyCalls {
        Dummy(DummyCall),
        ProxiableUUID(ProxiableUUIDCall),
        UpgradeToAndCall(UpgradeToAndCallCall),
    }
    impl ::ethers::core::abi::AbiDecode for DummyCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <DummyCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::Dummy(decoded));
            }
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
    impl ::ethers::core::abi::AbiEncode for DummyCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::Dummy(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ProxiableUUID(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::UpgradeToAndCall(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for DummyCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::Dummy(element) => ::core::fmt::Display::fmt(element, f),
                Self::ProxiableUUID(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpgradeToAndCall(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<DummyCall> for DummyCalls {
        fn from(value: DummyCall) -> Self {
            Self::Dummy(value)
        }
    }
    impl ::core::convert::From<ProxiableUUIDCall> for DummyCalls {
        fn from(value: ProxiableUUIDCall) -> Self {
            Self::ProxiableUUID(value)
        }
    }
    impl ::core::convert::From<UpgradeToAndCallCall> for DummyCalls {
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
