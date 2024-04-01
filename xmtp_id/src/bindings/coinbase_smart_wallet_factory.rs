pub use coinbase_smart_wallet_factory::*;
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
pub mod coinbase_smart_wallet_factory {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::Some(::ethers::core::abi::ethabi::Constructor {
                inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                    name: ::std::borrow::ToOwned::to_owned("erc4337"),
                    kind: ::ethers::core::abi::ethabi::ParamType::Address,
                    internal_type: ::core::option::Option::Some(::std::borrow::ToOwned::to_owned(
                        "address"
                    ),),
                },],
            }),
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("createAccount"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("createAccount"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("owners"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("nonce"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("account"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("contract CoinbaseSmartWallet",),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("getAddress"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("getAddress"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("owners"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Array(
                                    ::std::boxed::Box::new(
                                        ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    ),
                                ),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes[]"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("nonce"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("predicted"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("implementation"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("implementation"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("initCodeHash"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("initCodeHash"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("result"),
                            kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes32"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::core::convert::From::from([(
                ::std::borrow::ToOwned::to_owned("OwnerRequired"),
                ::std::vec![::ethers::core::abi::ethabi::AbiError {
                    name: ::std::borrow::ToOwned::to_owned("OwnerRequired"),
                    inputs: ::std::vec![],
                },],
            )]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static COINBASESMARTWALLETFACTORY_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\xA0`@R`@Qa\x05\xEB8\x03\x80a\x05\xEB\x839\x81\x01`@\x81\x90Ra\0\"\x91a\x003V[`\x01`\x01`\xA0\x1B\x03\x16`\x80Ra\0cV[`\0` \x82\x84\x03\x12\x15a\0EW`\0\x80\xFD[\x81Q`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\0\\W`\0\x80\xFD[\x93\x92PPPV[`\x80Qa\x05`a\0\x8B`\09`\0\x81\x81`\xA6\x01R\x81\x81a\x01<\x01Ra\x02;\x01Ra\x05``\0\xF3\xFE`\x80`@R`\x046\x10a\0?W`\x005`\xE0\x1C\x80c%\x0B\x1BA\x14a\0DW\x80c?\xFB\xA3o\x14a\0\x81W\x80c\\`\xDA\x1B\x14a\0\x94W\x80c\xDBLT^\x14a\0\xC8W[`\0\x80\xFD[4\x80\x15a\0PW`\0\x80\xFD[Pa\0da\0_6`\x04a\x03\xB7V[a\0\xEBV[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[a\0da\0\x8F6`\x04a\x03\xB7V[a\x01\x11V[4\x80\x15a\0\xA0W`\0\x80\xFD[Pa\0d\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81V[4\x80\x15a\0\xD4W`\0\x80\xFD[Pa\0\xDDa\x01\xE6V[`@Q\x90\x81R` \x01a\0xV[`\0a\x01\ta\0\xF8a\x01\xE6V[a\x01\x03\x86\x86\x86a\x02{V[0a\x02\xB1V[\x94\x93PPPPV[`\0\x82\x81\x03a\x013W`@Qc<wk\xE1`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\0\x80a\x01k4\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x01f\x89\x89\x89a\x02{V[a\x02\xD3V[\x93P\x91P\x82\x90P\x81\x15\x15`\0\x03a\x01\xDDW`@Qc7\x96\xF3\x87`\xE1\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x84\x16\x90co-\xE7\x0E\x90a\x01\xAA\x90\x89\x90\x89\x90`\x04\x01a\x04\xF2V[`\0`@Q\x80\x83\x03\x81`\0\x87\x80;\x15\x80\x15a\x01\xC4W`\0\x80\xFD[PZ\xF1\x15\x80\x15a\x01\xD8W=`\0\x80>=`\0\xFD[PPPP[PP\x93\x92PPPV[`@\x80Q\x7F\xCC75\xA9 \xA3\xCAP]8+\xBCTZ\xF4=`\0\x80>`8W=`\0\xFD[=`\0\xF3``\x90\x81R\x7FQU\xF36==7==6=\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v\x83Ra`\t` R\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0`\x1ERh`==\x81`\"=9s`\nR`_`! \x91\x90\x92R`\0\x90\x91R\x90V[`\0\x83\x83\x83`@Q` \x01a\x02\x92\x93\x92\x91\x90a\x05\x06V[`@Q` \x81\x83\x03\x03\x81R\x90`@R\x80Q\x90` \x01 \x90P\x93\x92PPPV[`\0`\xFF`\0SP`5\x92\x83R``\x1B`\x01R`\x15R`U`\0\x90\x81 \x91R\x90V[`\0\x80`@Q\x7F\xCC75\xA9 \xA3\xCAP]8+\xBCTZ\xF4=`\0\x80>`8W=`\0\xFD[=`\0\xF3``R\x7FQU\xF36==7==6=\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v`@Ra`\t` R\x84`\x1ERh`==\x81`\"=9s`\nR`_`! `5\x82\x01R0`X\x1B\x81R`\xFF\x81S\x83`\x15\x82\x01R`U\x81 \x91P\x81;a\x03\x7FW\x83`_`!\x88\xF5\x91P\x81a\x03zWc0\x11d%`\0R`\x04`\x1C\xFD[a\x03\xA5V[`\x01\x92P\x85\x15a\x03\xA5W`\08`\08\x89\x86Z\xF1a\x03\xA5Wc\xB1-\x13\xEB`\0R`\x04`\x1C\xFD[\x80`@RP`\0``R\x93P\x93\x91PPV[`\0\x80`\0`@\x84\x86\x03\x12\x15a\x03\xCCW`\0\x80\xFD[\x835g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x03\xE4W`\0\x80\xFD[\x81\x86\x01\x91P\x86`\x1F\x83\x01\x12a\x03\xF8W`\0\x80\xFD[\x815\x81\x81\x11\x15a\x04\x07W`\0\x80\xFD[\x87` \x82`\x05\x1B\x85\x01\x01\x11\x15a\x04\x1CW`\0\x80\xFD[` \x92\x83\x01\x98\x90\x97P\x95\x90\x91\x015\x94\x93PPPPV[\x81\x83R\x81\x81` \x85\x017P`\0\x82\x82\x01` \x90\x81\x01\x91\x90\x91R`\x1F\x90\x91\x01`\x1F\x19\x16\x90\x91\x01\x01\x90V[`\0\x83\x83\x85R` \x80\x86\x01\x95P` \x85`\x05\x1B\x83\x01\x01\x84`\0[\x87\x81\x10\x15a\x04\xE5W\x84\x83\x03`\x1F\x19\x01\x89R\x8156\x88\x90\x03`\x1E\x19\x01\x81\x12a\x04\x9BW`\0\x80\xFD[\x87\x01\x84\x81\x01\x905g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x04\xB7W`\0\x80\xFD[\x806\x03\x82\x13\x15a\x04\xC6W`\0\x80\xFD[a\x04\xD1\x85\x82\x84a\x042V[\x9A\x86\x01\x9A\x94PPP\x90\x83\x01\x90`\x01\x01a\x04uV[P\x90\x97\x96PPPPPPPV[` \x81R`\0a\x01\t` \x83\x01\x84\x86a\x04[V[`@\x81R`\0a\x05\x1A`@\x83\x01\x85\x87a\x04[V[\x90P\x82` \x83\x01R\x94\x93PPPPV\xFE\xA2dipfsX\"\x12 \x98\xBA\xE6Nb\x85\x9A\xC8\xD5\xED\x01\xC4\x92~_\xCE@oc+Q|\x86\xF08\xF0o\xEF\x83U\xDB\xA1dsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static COINBASESMARTWALLETFACTORY_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10a\0?W`\x005`\xE0\x1C\x80c%\x0B\x1BA\x14a\0DW\x80c?\xFB\xA3o\x14a\0\x81W\x80c\\`\xDA\x1B\x14a\0\x94W\x80c\xDBLT^\x14a\0\xC8W[`\0\x80\xFD[4\x80\x15a\0PW`\0\x80\xFD[Pa\0da\0_6`\x04a\x03\xB7V[a\0\xEBV[`@Q`\x01`\x01`\xA0\x1B\x03\x90\x91\x16\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[a\0da\0\x8F6`\x04a\x03\xB7V[a\x01\x11V[4\x80\x15a\0\xA0W`\0\x80\xFD[Pa\0d\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x81V[4\x80\x15a\0\xD4W`\0\x80\xFD[Pa\0\xDDa\x01\xE6V[`@Q\x90\x81R` \x01a\0xV[`\0a\x01\ta\0\xF8a\x01\xE6V[a\x01\x03\x86\x86\x86a\x02{V[0a\x02\xB1V[\x94\x93PPPPV[`\0\x82\x81\x03a\x013W`@Qc<wk\xE1`\xE0\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[`\0\x80a\x01k4\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0a\x01f\x89\x89\x89a\x02{V[a\x02\xD3V[\x93P\x91P\x82\x90P\x81\x15\x15`\0\x03a\x01\xDDW`@Qc7\x96\xF3\x87`\xE1\x1B\x81R`\x01`\x01`\xA0\x1B\x03\x84\x16\x90co-\xE7\x0E\x90a\x01\xAA\x90\x89\x90\x89\x90`\x04\x01a\x04\xF2V[`\0`@Q\x80\x83\x03\x81`\0\x87\x80;\x15\x80\x15a\x01\xC4W`\0\x80\xFD[PZ\xF1\x15\x80\x15a\x01\xD8W=`\0\x80>=`\0\xFD[PPPP[PP\x93\x92PPPV[`@\x80Q\x7F\xCC75\xA9 \xA3\xCAP]8+\xBCTZ\xF4=`\0\x80>`8W=`\0\xFD[=`\0\xF3``\x90\x81R\x7FQU\xF36==7==6=\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v\x83Ra`\t` R\x7F\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0`\x1ERh`==\x81`\"=9s`\nR`_`! \x91\x90\x92R`\0\x90\x91R\x90V[`\0\x83\x83\x83`@Q` \x01a\x02\x92\x93\x92\x91\x90a\x05\x06V[`@Q` \x81\x83\x03\x03\x81R\x90`@R\x80Q\x90` \x01 \x90P\x93\x92PPPV[`\0`\xFF`\0SP`5\x92\x83R``\x1B`\x01R`\x15R`U`\0\x90\x81 \x91R\x90V[`\0\x80`@Q\x7F\xCC75\xA9 \xA3\xCAP]8+\xBCTZ\xF4=`\0\x80>`8W=`\0\xFD[=`\0\xF3``R\x7FQU\xF36==7==6=\x7F6\x08\x94\xA1;\xA1\xA3!\x06g\xC8(I-\xB9\x8D\xCA> v`@Ra`\t` R\x84`\x1ERh`==\x81`\"=9s`\nR`_`! `5\x82\x01R0`X\x1B\x81R`\xFF\x81S\x83`\x15\x82\x01R`U\x81 \x91P\x81;a\x03\x7FW\x83`_`!\x88\xF5\x91P\x81a\x03zWc0\x11d%`\0R`\x04`\x1C\xFD[a\x03\xA5V[`\x01\x92P\x85\x15a\x03\xA5W`\08`\08\x89\x86Z\xF1a\x03\xA5Wc\xB1-\x13\xEB`\0R`\x04`\x1C\xFD[\x80`@RP`\0``R\x93P\x93\x91PPV[`\0\x80`\0`@\x84\x86\x03\x12\x15a\x03\xCCW`\0\x80\xFD[\x835g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x03\xE4W`\0\x80\xFD[\x81\x86\x01\x91P\x86`\x1F\x83\x01\x12a\x03\xF8W`\0\x80\xFD[\x815\x81\x81\x11\x15a\x04\x07W`\0\x80\xFD[\x87` \x82`\x05\x1B\x85\x01\x01\x11\x15a\x04\x1CW`\0\x80\xFD[` \x92\x83\x01\x98\x90\x97P\x95\x90\x91\x015\x94\x93PPPPV[\x81\x83R\x81\x81` \x85\x017P`\0\x82\x82\x01` \x90\x81\x01\x91\x90\x91R`\x1F\x90\x91\x01`\x1F\x19\x16\x90\x91\x01\x01\x90V[`\0\x83\x83\x85R` \x80\x86\x01\x95P` \x85`\x05\x1B\x83\x01\x01\x84`\0[\x87\x81\x10\x15a\x04\xE5W\x84\x83\x03`\x1F\x19\x01\x89R\x8156\x88\x90\x03`\x1E\x19\x01\x81\x12a\x04\x9BW`\0\x80\xFD[\x87\x01\x84\x81\x01\x905g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x04\xB7W`\0\x80\xFD[\x806\x03\x82\x13\x15a\x04\xC6W`\0\x80\xFD[a\x04\xD1\x85\x82\x84a\x042V[\x9A\x86\x01\x9A\x94PPP\x90\x83\x01\x90`\x01\x01a\x04uV[P\x90\x97\x96PPPPPPPV[` \x81R`\0a\x01\t` \x83\x01\x84\x86a\x04[V[`@\x81R`\0a\x05\x1A`@\x83\x01\x85\x87a\x04[V[\x90P\x82` \x83\x01R\x94\x93PPPPV\xFE\xA2dipfsX\"\x12 \x98\xBA\xE6Nb\x85\x9A\xC8\xD5\xED\x01\xC4\x92~_\xCE@oc+Q|\x86\xF08\xF0o\xEF\x83U\xDB\xA1dsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static COINBASESMARTWALLETFACTORY_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct CoinbaseSmartWalletFactory<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for CoinbaseSmartWalletFactory<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for CoinbaseSmartWalletFactory<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for CoinbaseSmartWalletFactory<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for CoinbaseSmartWalletFactory<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(CoinbaseSmartWalletFactory))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> CoinbaseSmartWalletFactory<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                COINBASESMARTWALLETFACTORY_ABI.clone(),
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
                COINBASESMARTWALLETFACTORY_ABI.clone(),
                COINBASESMARTWALLETFACTORY_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `createAccount` (0x3ffba36f) function
        pub fn create_account(
            &self,
            owners: ::std::vec::Vec<::ethers::core::types::Bytes>,
            nonce: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([63, 251, 163, 111], (owners, nonce))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `getAddress` (0x250b1b41) function
        pub fn get_address(
            &self,
            owners: ::std::vec::Vec<::ethers::core::types::Bytes>,
            nonce: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([37, 11, 27, 65], (owners, nonce))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `implementation` (0x5c60da1b) function
        pub fn implementation(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([92, 96, 218, 27], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `initCodeHash` (0xdb4c545e) function
        pub fn init_code_hash(&self) -> ::ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([219, 76, 84, 94], ())
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for CoinbaseSmartWalletFactory<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `OwnerRequired` with signature `OwnerRequired()` and selector `0x3c776be1`
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
    #[etherror(name = "OwnerRequired", abi = "OwnerRequired()")]
    pub struct OwnerRequired;
    ///Container type for all input parameters for the `createAccount` function with signature `createAccount(bytes[],uint256)` and selector `0x3ffba36f`
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
    #[ethcall(name = "createAccount", abi = "createAccount(bytes[],uint256)")]
    pub struct CreateAccountCall {
        pub owners: ::std::vec::Vec<::ethers::core::types::Bytes>,
        pub nonce: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `getAddress` function with signature `getAddress(bytes[],uint256)` and selector `0x250b1b41`
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
    #[ethcall(name = "getAddress", abi = "getAddress(bytes[],uint256)")]
    pub struct GetAddressCall {
        pub owners: ::std::vec::Vec<::ethers::core::types::Bytes>,
        pub nonce: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `implementation` function with signature `implementation()` and selector `0x5c60da1b`
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
    #[ethcall(name = "implementation", abi = "implementation()")]
    pub struct ImplementationCall;
    ///Container type for all input parameters for the `initCodeHash` function with signature `initCodeHash()` and selector `0xdb4c545e`
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
    #[ethcall(name = "initCodeHash", abi = "initCodeHash()")]
    pub struct InitCodeHashCall;
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
    pub enum CoinbaseSmartWalletFactoryCalls {
        CreateAccount(CreateAccountCall),
        GetAddress(GetAddressCall),
        Implementation(ImplementationCall),
        InitCodeHash(InitCodeHashCall),
    }
    impl ::ethers::core::abi::AbiDecode for CoinbaseSmartWalletFactoryCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <CreateAccountCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::CreateAccount(decoded));
            }
            if let Ok(decoded) = <GetAddressCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::GetAddress(decoded));
            }
            if let Ok(decoded) =
                <ImplementationCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::Implementation(decoded));
            }
            if let Ok(decoded) = <InitCodeHashCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::InitCodeHash(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for CoinbaseSmartWalletFactoryCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::CreateAccount(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GetAddress(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::Implementation(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::InitCodeHash(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for CoinbaseSmartWalletFactoryCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::CreateAccount(element) => ::core::fmt::Display::fmt(element, f),
                Self::GetAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::Implementation(element) => ::core::fmt::Display::fmt(element, f),
                Self::InitCodeHash(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<CreateAccountCall> for CoinbaseSmartWalletFactoryCalls {
        fn from(value: CreateAccountCall) -> Self {
            Self::CreateAccount(value)
        }
    }
    impl ::core::convert::From<GetAddressCall> for CoinbaseSmartWalletFactoryCalls {
        fn from(value: GetAddressCall) -> Self {
            Self::GetAddress(value)
        }
    }
    impl ::core::convert::From<ImplementationCall> for CoinbaseSmartWalletFactoryCalls {
        fn from(value: ImplementationCall) -> Self {
            Self::Implementation(value)
        }
    }
    impl ::core::convert::From<InitCodeHashCall> for CoinbaseSmartWalletFactoryCalls {
        fn from(value: InitCodeHashCall) -> Self {
            Self::InitCodeHash(value)
        }
    }
    ///Container type for all return fields from the `createAccount` function with signature `createAccount(bytes[],uint256)` and selector `0x3ffba36f`
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
    pub struct CreateAccountReturn {
        pub account: ::ethers::core::types::Address,
    }
    ///Container type for all return fields from the `getAddress` function with signature `getAddress(bytes[],uint256)` and selector `0x250b1b41`
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
    pub struct GetAddressReturn {
        pub predicted: ::ethers::core::types::Address,
    }
    ///Container type for all return fields from the `implementation` function with signature `implementation()` and selector `0x5c60da1b`
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
    pub struct ImplementationReturn(pub ::ethers::core::types::Address);
    ///Container type for all return fields from the `initCodeHash` function with signature `initCodeHash()` and selector `0xdb4c545e`
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
    pub struct InitCodeHashReturn {
        pub result: [u8; 32],
    }
}
