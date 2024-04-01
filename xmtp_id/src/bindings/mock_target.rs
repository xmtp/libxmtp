pub use mock_target::*;
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
pub mod mock_target {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("changeOwnerSlotValue"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("changeOwnerSlotValue",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("change"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bool"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("data"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("data"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
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
                    ::std::borrow::ToOwned::to_owned("datahash"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("datahash"),
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
                    ::std::borrow::ToOwned::to_owned("revertWithTargetError"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("revertWithTargetError",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("data_"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("setData"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("setData"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("data_"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([(
                ::std::borrow::ToOwned::to_owned("TargetError"),
                ::std::vec![::ethers::core::abi::ethabi::AbiError {
                    name: ::std::borrow::ToOwned::to_owned("TargetError"),
                    inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                        name: ::std::borrow::ToOwned::to_owned("data"),
                        kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                        internal_type: ::core::option::Option::Some(
                            ::std::borrow::ToOwned::to_owned("bytes"),
                        ),
                    },],
                },],
            )]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static MOCKTARGET_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x10W`\0\x80\xFD[Pa\x04y\x80a\0 `\09`\0\xF3\xFE`\x80`@R`\x046\x10a\0JW`\x005`\xE0\x1C\x80c\x0CO`Y\x14a\0OW\x80c'k\x86\xA9\x14a\0dW\x80ca\xA3\x0B.\x14a\0\x8DW\x80cs\xD4\xA1:\x14a\0\xA0W\x80c\xABb\xF0\xE1\x14a\0\xC2W[`\0\x80\xFD[a\0ba\0]6`\x04a\x01\xCFV[a\0\xD5V[\0[4\x80\x15a\0pW`\0\x80\xFD[Pa\0z`\0T\x81V[`@Q\x90\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[a\0ba\0\x9B6`\x04a\x02\x80V[a\0\xF9V[4\x80\x15a\0\xACW`\0\x80\xFD[Pa\0\xB5a\x01\x0EV[`@Qa\0\x84\x91\x90a\x02\xA9V[a\0\xB5a\0\xD06`\x04a\x01\xCFV[a\x01\x9CV[\x80`@Qc4>\xB9q`\xE1\x1B\x81R`\x04\x01a\0\xF0\x91\x90a\x02\xA9V[`@Q\x80\x91\x03\x90\xFD[\x80\x15a\x01\x0BWb\x11\"3c\x8Bx\xC6\xD8\x19U[PV[`\x01\x80Ta\x01\x1B\x90a\x02\xF8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x01G\x90a\x02\xF8V[\x80\x15a\x01\x94W\x80`\x1F\x10a\x01iWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x01\x94V[\x82\x01\x91\x90`\0R` `\0 \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x01wW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81V[```\x01a\x01\xAA\x83\x82a\x03\x83V[PP\x80Q` \x82\x01 `\0U\x90V[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`\0` \x82\x84\x03\x12\x15a\x01\xE1W`\0\x80\xFD[\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x01\xF9W`\0\x80\xFD[\x81\x84\x01\x91P\x84`\x1F\x83\x01\x12a\x02\rW`\0\x80\xFD[\x815\x81\x81\x11\x15a\x02\x1FWa\x02\x1Fa\x01\xB9V[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x83\x82\x11\x81\x83\x10\x17\x15a\x02GWa\x02Ga\x01\xB9V[\x81`@R\x82\x81R\x87` \x84\x87\x01\x01\x11\x15a\x02`W`\0\x80\xFD[\x82` \x86\x01` \x83\x017`\0\x92\x81\x01` \x01\x92\x90\x92RP\x95\x94PPPPPV[`\0` \x82\x84\x03\x12\x15a\x02\x92W`\0\x80\xFD[\x815\x80\x15\x15\x81\x14a\x02\xA2W`\0\x80\xFD[\x93\x92PPPV[`\0` \x80\x83R\x83Q\x80` \x85\x01R`\0[\x81\x81\x10\x15a\x02\xD7W\x85\x81\x01\x83\x01Q\x85\x82\x01`@\x01R\x82\x01a\x02\xBBV[P`\0`@\x82\x86\x01\x01R`@`\x1F\x19`\x1F\x83\x01\x16\x85\x01\x01\x92PPP\x92\x91PPV[`\x01\x81\x81\x1C\x90\x82\x16\x80a\x03\x0CW`\x7F\x82\x16\x91P[` \x82\x10\x81\x03a\x03,WcNH{q`\xE0\x1B`\0R`\"`\x04R`$`\0\xFD[P\x91\x90PV[`\x1F\x82\x11\x15a\x03~W`\0\x81`\0R` `\0 `\x1F\x85\x01`\x05\x1C\x81\x01` \x86\x10\x15a\x03[WP\x80[`\x1F\x85\x01`\x05\x1C\x82\x01\x91P[\x81\x81\x10\x15a\x03zW\x82\x81U`\x01\x01a\x03gV[PPP[PPPV[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x03\x9DWa\x03\x9Da\x01\xB9V[a\x03\xB1\x81a\x03\xAB\x84Ta\x02\xF8V[\x84a\x032V[` \x80`\x1F\x83\x11`\x01\x81\x14a\x03\xE6W`\0\x84\x15a\x03\xCEWP\x85\x83\x01Q[`\0\x19`\x03\x86\x90\x1B\x1C\x19\x16`\x01\x85\x90\x1B\x17\x85Ua\x03zV[`\0\x85\x81R` \x81 `\x1F\x19\x86\x16\x91[\x82\x81\x10\x15a\x04\x15W\x88\x86\x01Q\x82U\x94\x84\x01\x94`\x01\x90\x91\x01\x90\x84\x01a\x03\xF6V[P\x85\x82\x10\x15a\x043W\x87\x85\x01Q`\0\x19`\x03\x88\x90\x1B`\xF8\x16\x1C\x19\x16\x81U[PPPPP`\x01\x90\x81\x1B\x01\x90UPV\xFE\xA2dipfsX\"\x12 G\x92L\xC3U\x8F^\xDF\x01\xB8\x0B\x8Ee\x08\xB6\x8B\xBB\xE6\x12p\xFE\x0F6~\x864\xAC\xEB\xF7\"e\xBEdsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static MOCKTARGET_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10a\0JW`\x005`\xE0\x1C\x80c\x0CO`Y\x14a\0OW\x80c'k\x86\xA9\x14a\0dW\x80ca\xA3\x0B.\x14a\0\x8DW\x80cs\xD4\xA1:\x14a\0\xA0W\x80c\xABb\xF0\xE1\x14a\0\xC2W[`\0\x80\xFD[a\0ba\0]6`\x04a\x01\xCFV[a\0\xD5V[\0[4\x80\x15a\0pW`\0\x80\xFD[Pa\0z`\0T\x81V[`@Q\x90\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[a\0ba\0\x9B6`\x04a\x02\x80V[a\0\xF9V[4\x80\x15a\0\xACW`\0\x80\xFD[Pa\0\xB5a\x01\x0EV[`@Qa\0\x84\x91\x90a\x02\xA9V[a\0\xB5a\0\xD06`\x04a\x01\xCFV[a\x01\x9CV[\x80`@Qc4>\xB9q`\xE1\x1B\x81R`\x04\x01a\0\xF0\x91\x90a\x02\xA9V[`@Q\x80\x91\x03\x90\xFD[\x80\x15a\x01\x0BWb\x11\"3c\x8Bx\xC6\xD8\x19U[PV[`\x01\x80Ta\x01\x1B\x90a\x02\xF8V[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x01G\x90a\x02\xF8V[\x80\x15a\x01\x94W\x80`\x1F\x10a\x01iWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x01\x94V[\x82\x01\x91\x90`\0R` `\0 \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x01wW\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x81V[```\x01a\x01\xAA\x83\x82a\x03\x83V[PP\x80Q` \x82\x01 `\0U\x90V[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`\0` \x82\x84\x03\x12\x15a\x01\xE1W`\0\x80\xFD[\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x01\xF9W`\0\x80\xFD[\x81\x84\x01\x91P\x84`\x1F\x83\x01\x12a\x02\rW`\0\x80\xFD[\x815\x81\x81\x11\x15a\x02\x1FWa\x02\x1Fa\x01\xB9V[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x83\x82\x11\x81\x83\x10\x17\x15a\x02GWa\x02Ga\x01\xB9V[\x81`@R\x82\x81R\x87` \x84\x87\x01\x01\x11\x15a\x02`W`\0\x80\xFD[\x82` \x86\x01` \x83\x017`\0\x92\x81\x01` \x01\x92\x90\x92RP\x95\x94PPPPPV[`\0` \x82\x84\x03\x12\x15a\x02\x92W`\0\x80\xFD[\x815\x80\x15\x15\x81\x14a\x02\xA2W`\0\x80\xFD[\x93\x92PPPV[`\0` \x80\x83R\x83Q\x80` \x85\x01R`\0[\x81\x81\x10\x15a\x02\xD7W\x85\x81\x01\x83\x01Q\x85\x82\x01`@\x01R\x82\x01a\x02\xBBV[P`\0`@\x82\x86\x01\x01R`@`\x1F\x19`\x1F\x83\x01\x16\x85\x01\x01\x92PPP\x92\x91PPV[`\x01\x81\x81\x1C\x90\x82\x16\x80a\x03\x0CW`\x7F\x82\x16\x91P[` \x82\x10\x81\x03a\x03,WcNH{q`\xE0\x1B`\0R`\"`\x04R`$`\0\xFD[P\x91\x90PV[`\x1F\x82\x11\x15a\x03~W`\0\x81`\0R` `\0 `\x1F\x85\x01`\x05\x1C\x81\x01` \x86\x10\x15a\x03[WP\x80[`\x1F\x85\x01`\x05\x1C\x82\x01\x91P[\x81\x81\x10\x15a\x03zW\x82\x81U`\x01\x01a\x03gV[PPP[PPPV[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x03\x9DWa\x03\x9Da\x01\xB9V[a\x03\xB1\x81a\x03\xAB\x84Ta\x02\xF8V[\x84a\x032V[` \x80`\x1F\x83\x11`\x01\x81\x14a\x03\xE6W`\0\x84\x15a\x03\xCEWP\x85\x83\x01Q[`\0\x19`\x03\x86\x90\x1B\x1C\x19\x16`\x01\x85\x90\x1B\x17\x85Ua\x03zV[`\0\x85\x81R` \x81 `\x1F\x19\x86\x16\x91[\x82\x81\x10\x15a\x04\x15W\x88\x86\x01Q\x82U\x94\x84\x01\x94`\x01\x90\x91\x01\x90\x84\x01a\x03\xF6V[P\x85\x82\x10\x15a\x043W\x87\x85\x01Q`\0\x19`\x03\x88\x90\x1B`\xF8\x16\x1C\x19\x16\x81U[PPPPP`\x01\x90\x81\x1B\x01\x90UPV\xFE\xA2dipfsX\"\x12 G\x92L\xC3U\x8F^\xDF\x01\xB8\x0B\x8Ee\x08\xB6\x8B\xBB\xE6\x12p\xFE\x0F6~\x864\xAC\xEB\xF7\"e\xBEdsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static MOCKTARGET_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct MockTarget<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for MockTarget<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for MockTarget<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for MockTarget<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for MockTarget<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(MockTarget))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> MockTarget<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                MOCKTARGET_ABI.clone(),
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
                MOCKTARGET_ABI.clone(),
                MOCKTARGET_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `changeOwnerSlotValue` (0x61a30b2e) function
        pub fn change_owner_slot_value(
            &self,
            change: bool,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([97, 163, 11, 46], change)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `data` (0x73d4a13a) function
        pub fn data(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Bytes> {
            self.0
                .method_hash([115, 212, 161, 58], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `datahash` (0x276b86a9) function
        pub fn datahash(&self) -> ::ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([39, 107, 134, 169], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `revertWithTargetError` (0x0c4f6059) function
        pub fn revert_with_target_error(
            &self,
            data: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([12, 79, 96, 89], data)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `setData` (0xab62f0e1) function
        pub fn set_data(
            &self,
            data: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Bytes> {
            self.0
                .method_hash([171, 98, 240, 225], data)
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for MockTarget<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `TargetError` with signature `TargetError(bytes)` and selector `0x687d72e2`
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
    #[etherror(name = "TargetError", abi = "TargetError(bytes)")]
    pub struct TargetError {
        pub data: ::ethers::core::types::Bytes,
    }
    ///Container type for all input parameters for the `changeOwnerSlotValue` function with signature `changeOwnerSlotValue(bool)` and selector `0x61a30b2e`
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
    #[ethcall(name = "changeOwnerSlotValue", abi = "changeOwnerSlotValue(bool)")]
    pub struct ChangeOwnerSlotValueCall {
        pub change: bool,
    }
    ///Container type for all input parameters for the `data` function with signature `data()` and selector `0x73d4a13a`
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
    #[ethcall(name = "data", abi = "data()")]
    pub struct DataCall;
    ///Container type for all input parameters for the `datahash` function with signature `datahash()` and selector `0x276b86a9`
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
    #[ethcall(name = "datahash", abi = "datahash()")]
    pub struct DatahashCall;
    ///Container type for all input parameters for the `revertWithTargetError` function with signature `revertWithTargetError(bytes)` and selector `0x0c4f6059`
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
    #[ethcall(name = "revertWithTargetError", abi = "revertWithTargetError(bytes)")]
    pub struct RevertWithTargetErrorCall {
        pub data: ::ethers::core::types::Bytes,
    }
    ///Container type for all input parameters for the `setData` function with signature `setData(bytes)` and selector `0xab62f0e1`
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
    #[ethcall(name = "setData", abi = "setData(bytes)")]
    pub struct SetDataCall {
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
    pub enum MockTargetCalls {
        ChangeOwnerSlotValue(ChangeOwnerSlotValueCall),
        Data(DataCall),
        Datahash(DatahashCall),
        RevertWithTargetError(RevertWithTargetErrorCall),
        SetData(SetDataCall),
    }
    impl ::ethers::core::abi::AbiDecode for MockTargetCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <ChangeOwnerSlotValueCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ChangeOwnerSlotValue(decoded));
            }
            if let Ok(decoded) = <DataCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::Data(decoded));
            }
            if let Ok(decoded) = <DatahashCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::Datahash(decoded));
            }
            if let Ok(decoded) =
                <RevertWithTargetErrorCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertWithTargetError(decoded));
            }
            if let Ok(decoded) = <SetDataCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::SetData(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for MockTargetCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::ChangeOwnerSlotValue(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::Data(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::Datahash(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RevertWithTargetError(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::SetData(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for MockTargetCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::ChangeOwnerSlotValue(element) => ::core::fmt::Display::fmt(element, f),
                Self::Data(element) => ::core::fmt::Display::fmt(element, f),
                Self::Datahash(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertWithTargetError(element) => ::core::fmt::Display::fmt(element, f),
                Self::SetData(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<ChangeOwnerSlotValueCall> for MockTargetCalls {
        fn from(value: ChangeOwnerSlotValueCall) -> Self {
            Self::ChangeOwnerSlotValue(value)
        }
    }
    impl ::core::convert::From<DataCall> for MockTargetCalls {
        fn from(value: DataCall) -> Self {
            Self::Data(value)
        }
    }
    impl ::core::convert::From<DatahashCall> for MockTargetCalls {
        fn from(value: DatahashCall) -> Self {
            Self::Datahash(value)
        }
    }
    impl ::core::convert::From<RevertWithTargetErrorCall> for MockTargetCalls {
        fn from(value: RevertWithTargetErrorCall) -> Self {
            Self::RevertWithTargetError(value)
        }
    }
    impl ::core::convert::From<SetDataCall> for MockTargetCalls {
        fn from(value: SetDataCall) -> Self {
            Self::SetData(value)
        }
    }
    ///Container type for all return fields from the `data` function with signature `data()` and selector `0x73d4a13a`
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
    pub struct DataReturn(pub ::ethers::core::types::Bytes);
    ///Container type for all return fields from the `datahash` function with signature `datahash()` and selector `0x276b86a9`
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
    pub struct DatahashReturn(pub [u8; 32]);
    ///Container type for all return fields from the `setData` function with signature `setData(bytes)` and selector `0xab62f0e1`
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
    pub struct SetDataReturn(pub ::ethers::core::types::Bytes);
}
