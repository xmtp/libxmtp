pub use multi_ownable::*;
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
pub mod multi_ownable {
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("addOwnerAddress"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("addOwnerAddress"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("owner"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("addOwnerPublicKey"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("addOwnerPublicKey"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("x"),
                                kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes32"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("y"),
                                kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes32"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("isOwnerAddress"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("isOwnerAddress"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("account"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bool"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("isOwnerBytes"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("isOwnerBytes"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("account"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bool"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("isOwnerPublicKey"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("isOwnerPublicKey"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("x"),
                                kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes32"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("y"),
                                kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes32"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bool,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bool"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("nextOwnerIndex"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("nextOwnerIndex"),
                        inputs: ::std::vec![],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
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
                    ::std::borrow::ToOwned::to_owned("ownerAtIndex"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("ownerAtIndex"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("index"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
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
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::View,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("removeOwnerAtIndex"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("removeOwnerAtIndex"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("index"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::NonPayable,
                    },],
                ),
            ]),
            events: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AddOwner"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("AddOwner"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("index"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("owner"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("RemoveOwner"),
                    ::std::vec![::ethers::core::abi::ethabi::Event {
                        name: ::std::borrow::ToOwned::to_owned("RemoveOwner"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("index"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                indexed: true,
                            },
                            ::ethers::core::abi::ethabi::EventParam {
                                name: ::std::borrow::ToOwned::to_owned("owner"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                                indexed: false,
                            },
                        ],
                        anonymous: false,
                    },],
                ),
            ]),
            errors: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("AlreadyOwner"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("AlreadyOwner"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("owner"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("InvalidEthereumAddressOwner"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("InvalidEthereumAddressOwner",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("owner"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("InvalidOwnerBytesLength"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("InvalidOwnerBytesLength",),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("owner"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Bytes,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("bytes"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("NoOwnerAtIndex"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("NoOwnerAtIndex"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("index"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("Unauthorized"),
                    ::std::vec![::ethers::core::abi::ethabi::AbiError {
                        name: ::std::borrow::ToOwned::to_owned("Unauthorized"),
                        inputs: ::std::vec![],
                    },],
                ),
            ]),
            receive: false,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static MULTIOWNABLE_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x10W`\0\x80\xFD[Pa\t8\x80a\0 `\09`\0\xF3\xFE`\x80`@R4\x80\x15a\0\x10W`\0\x80\xFD[P`\x046\x10a\0\x88W`\x005`\xE0\x1C\x80cr\xDE;Z\x11a\0[W\x80cr\xDE;Z\x14a\0\xF0W\x80c\x8E\xA6\x90)\x14a\x01\x03W\x80c\xA2\xE1\xA8\xD8\x14a\x01#W\x80c\xD9H\xFD.\x14a\x016W`\0\x80\xFD[\x80c\x06j\x1E\xB7\x14a\0\x8DW\x80c\x0F\x0F?$\x14a\0\xB5W\x80c\x1C\xA59?\x14a\0\xCAW\x80c)V^;\x14a\0\xDDW[`\0\x80\xFD[a\0\xA0a\0\x9B6`\x04a\x05\xCBV[a\x01TV[`@Q\x90\x15\x15\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[a\0\xC8a\0\xC36`\x04a\x05\xEDV[a\x01\xC2V[\0[a\0\xA0a\0\xD86`\x04a\x063V[a\x01\xFAV[a\0\xC8a\0\xEB6`\x04a\x05\xCBV[a\x025V[a\0\xC8a\0\xFE6`\x04a\x06\xE4V[a\x02^V[a\x01\x16a\x01\x116`\x04a\x06\xE4V[a\x03PV[`@Qa\0\xAC\x91\x90a\x07!V[a\0\xA0a\x0116`\x04a\x05\xEDV[a\x04\x11V[`\0\x80Q` a\x08\xE3\x839\x81Q\x91RT`@Q\x90\x81R` \x01a\0\xACV[`@\x80Q` \x81\x01\x84\x90R\x90\x81\x01\x82\x90R`\0\x90\x7F\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\x02\x90``\x01`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R\x90\x82\x90Ra\x01\xA7\x91a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 T`\xFF\x16\x90P\x92\x91PPV[a\x01\xCAa\x04WV[`@\x80Q`\x01`\x01`\xA0\x1B\x03\x83\x16` \x82\x01Ra\x01\xF7\x91\x01[`@Q` \x81\x83\x03\x03\x81R\x90`@Ra\x04\x89V[PV[`\0`\0\x80Q` a\x08\xE3\x839\x81Q\x91R`\x02\x01\x82`@Qa\x02\x1C\x91\x90a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 T`\xFF\x16\x92\x91PPV[a\x02=a\x04WV[`@\x80Q` \x81\x01\x84\x90R\x90\x81\x01\x82\x90Ra\x02Z\x90``\x01a\x01\xE3V[PPV[a\x02fa\x04WV[`\0a\x02q\x82a\x03PV[\x90P\x80Q`\0\x03a\x02\x9DW`@Qc4\x0CG=`\xE1\x1B\x81R`\x04\x81\x01\x83\x90R`$\x01[`@Q\x80\x91\x03\x90\xFD[`@Q\x7F\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\x02\x90a\x02\xCD\x90\x83\x90a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 \x80T`\xFF\x19\x16\x90Ua\x02\xF9`\0\x80Q` a\x08\xE3\x839\x81Q\x91R\x90V[`\0\x83\x81R`\x01\x91\x90\x91\x01` R`@\x81 a\x03\x14\x91a\x05}V[\x81\x7F\xCF\x95\xBB\xFEo\x87\x0F\x8C\xC4\x04\x82\xDC=\xCC\xDA\xFD&\x8F\x0E\x9C\xE0\xA4\xF2N\xA1\xBE\xA9\xBEd\xE5\x05\xFF\x82`@Qa\x03D\x91\x90a\x07!V[`@Q\x80\x91\x03\x90\xA2PPV[`\0\x81\x81R\x7F\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\x01` R`@\x90 \x80T``\x91\x90a\x03\x8C\x90a\x07pV[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x03\xB8\x90a\x07pV[\x80\x15a\x04\x05W\x80`\x1F\x10a\x03\xDAWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x04\x05V[\x82\x01\x91\x90`\0R` `\0 \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x03\xE8W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x90P\x91\x90PV[`\0`\0\x80Q` a\x08\xE3\x839\x81Q\x91R`@\x80Q`\x01`\x01`\xA0\x1B\x03\x85\x16` \x82\x01R`\x02\x92\x90\x92\x01\x91\x01`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R\x90\x82\x90Ra\x02\x1C\x91a\x07TV[a\x04`3a\x04\x11V[\x80a\x04jWP30\x14[\x15a\x04qWV[`@Qb\x82\xB4)`\xE8\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x01\xF7\x81`\0\x80Q` a\x08\xE3\x839\x81Q\x91R\x80T\x90`\0a\x04\xAA\x83a\x07\xAAV[\x91\x90PUa\x04\xB7\x82a\x01\xFAV[\x15a\x04\xD7W\x81`@QcF\x8B\x12\xAD`\xE1\x1B\x81R`\x04\x01a\x02\x94\x91\x90a\x07!V[`\x01`\0\x80Q` a\x08\xE3\x839\x81Q\x91R`\x02\x01\x83`@Qa\x04\xF9\x91\x90a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 \x80T\x91\x15\x15`\xFF\x19\x90\x92\x16\x91\x90\x91\x17\x90U\x81a\x05/`\0\x80Q` a\x08\xE3\x839\x81Q\x91R\x90V[`\0\x83\x81R`\x01\x91\x90\x91\x01` R`@\x90 \x90a\x05L\x90\x82a\x08\"V[P\x80\x7F8\x10\x9E\xDC&\xE1f\xB5W\x93R\xCEV\xA5\x08\x13\x17~\xB2R\x08\xFD\x90\xD6\x1F/7\x83\x86\"\x02 \x83`@Qa\x03D\x91\x90a\x07!V[P\x80Ta\x05\x89\x90a\x07pV[`\0\x82U\x80`\x1F\x10a\x05\x99WPPV[`\x1F\x01` \x90\x04\x90`\0R` `\0 \x90\x81\x01\x90a\x01\xF7\x91\x90[\x80\x82\x11\x15a\x05\xC7W`\0\x81U`\x01\x01a\x05\xB3V[P\x90V[`\0\x80`@\x83\x85\x03\x12\x15a\x05\xDEW`\0\x80\xFD[PP\x805\x92` \x90\x91\x015\x91PV[`\0` \x82\x84\x03\x12\x15a\x05\xFFW`\0\x80\xFD[\x815`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x06\x16W`\0\x80\xFD[\x93\x92PPPV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`\0` \x82\x84\x03\x12\x15a\x06EW`\0\x80\xFD[\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x06]W`\0\x80\xFD[\x81\x84\x01\x91P\x84`\x1F\x83\x01\x12a\x06qW`\0\x80\xFD[\x815\x81\x81\x11\x15a\x06\x83Wa\x06\x83a\x06\x1DV[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x83\x82\x11\x81\x83\x10\x17\x15a\x06\xABWa\x06\xABa\x06\x1DV[\x81`@R\x82\x81R\x87` \x84\x87\x01\x01\x11\x15a\x06\xC4W`\0\x80\xFD[\x82` \x86\x01` \x83\x017`\0\x92\x81\x01` \x01\x92\x90\x92RP\x95\x94PPPPPV[`\0` \x82\x84\x03\x12\x15a\x06\xF6W`\0\x80\xFD[P5\x91\x90PV[`\0[\x83\x81\x10\x15a\x07\x18W\x81\x81\x01Q\x83\x82\x01R` \x01a\x07\0V[PP`\0\x91\x01RV[` \x81R`\0\x82Q\x80` \x84\x01Ra\x07@\x81`@\x85\x01` \x87\x01a\x06\xFDV[`\x1F\x01`\x1F\x19\x16\x91\x90\x91\x01`@\x01\x92\x91PPV[`\0\x82Qa\x07f\x81\x84` \x87\x01a\x06\xFDV[\x91\x90\x91\x01\x92\x91PPV[`\x01\x81\x81\x1C\x90\x82\x16\x80a\x07\x84W`\x7F\x82\x16\x91P[` \x82\x10\x81\x03a\x07\xA4WcNH{q`\xE0\x1B`\0R`\"`\x04R`$`\0\xFD[P\x91\x90PV[`\0`\x01\x82\x01a\x07\xCAWcNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[P`\x01\x01\x90V[`\x1F\x82\x11\x15a\x08\x1DW`\0\x81`\0R` `\0 `\x1F\x85\x01`\x05\x1C\x81\x01` \x86\x10\x15a\x07\xFAWP\x80[`\x1F\x85\x01`\x05\x1C\x82\x01\x91P[\x81\x81\x10\x15a\x08\x19W\x82\x81U`\x01\x01a\x08\x06V[PPP[PPPV[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x08<Wa\x08<a\x06\x1DV[a\x08P\x81a\x08J\x84Ta\x07pV[\x84a\x07\xD1V[` \x80`\x1F\x83\x11`\x01\x81\x14a\x08\x85W`\0\x84\x15a\x08mWP\x85\x83\x01Q[`\0\x19`\x03\x86\x90\x1B\x1C\x19\x16`\x01\x85\x90\x1B\x17\x85Ua\x08\x19V[`\0\x85\x81R` \x81 `\x1F\x19\x86\x16\x91[\x82\x81\x10\x15a\x08\xB4W\x88\x86\x01Q\x82U\x94\x84\x01\x94`\x01\x90\x91\x01\x90\x84\x01a\x08\x95V[P\x85\x82\x10\x15a\x08\xD2W\x87\x85\x01Q`\0\x19`\x03\x88\x90\x1B`\xF8\x16\x1C\x19\x16\x81U[PPPPP`\x01\x90\x81\x1B\x01\x90UPV\xFE\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\0\xA2dipfsX\"\x12 \xBC\xE0j\x887K\x02\xFE<\x84\x04\x9C>I[gTmvF\xF0J\xDE\x10\xDD@\x17\xA7*\x8B2\xD7dsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static MULTIOWNABLE_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x10W`\0\x80\xFD[P`\x046\x10a\0\x88W`\x005`\xE0\x1C\x80cr\xDE;Z\x11a\0[W\x80cr\xDE;Z\x14a\0\xF0W\x80c\x8E\xA6\x90)\x14a\x01\x03W\x80c\xA2\xE1\xA8\xD8\x14a\x01#W\x80c\xD9H\xFD.\x14a\x016W`\0\x80\xFD[\x80c\x06j\x1E\xB7\x14a\0\x8DW\x80c\x0F\x0F?$\x14a\0\xB5W\x80c\x1C\xA59?\x14a\0\xCAW\x80c)V^;\x14a\0\xDDW[`\0\x80\xFD[a\0\xA0a\0\x9B6`\x04a\x05\xCBV[a\x01TV[`@Q\x90\x15\x15\x81R` \x01[`@Q\x80\x91\x03\x90\xF3[a\0\xC8a\0\xC36`\x04a\x05\xEDV[a\x01\xC2V[\0[a\0\xA0a\0\xD86`\x04a\x063V[a\x01\xFAV[a\0\xC8a\0\xEB6`\x04a\x05\xCBV[a\x025V[a\0\xC8a\0\xFE6`\x04a\x06\xE4V[a\x02^V[a\x01\x16a\x01\x116`\x04a\x06\xE4V[a\x03PV[`@Qa\0\xAC\x91\x90a\x07!V[a\0\xA0a\x0116`\x04a\x05\xEDV[a\x04\x11V[`\0\x80Q` a\x08\xE3\x839\x81Q\x91RT`@Q\x90\x81R` \x01a\0\xACV[`@\x80Q` \x81\x01\x84\x90R\x90\x81\x01\x82\x90R`\0\x90\x7F\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\x02\x90``\x01`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R\x90\x82\x90Ra\x01\xA7\x91a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 T`\xFF\x16\x90P\x92\x91PPV[a\x01\xCAa\x04WV[`@\x80Q`\x01`\x01`\xA0\x1B\x03\x83\x16` \x82\x01Ra\x01\xF7\x91\x01[`@Q` \x81\x83\x03\x03\x81R\x90`@Ra\x04\x89V[PV[`\0`\0\x80Q` a\x08\xE3\x839\x81Q\x91R`\x02\x01\x82`@Qa\x02\x1C\x91\x90a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 T`\xFF\x16\x92\x91PPV[a\x02=a\x04WV[`@\x80Q` \x81\x01\x84\x90R\x90\x81\x01\x82\x90Ra\x02Z\x90``\x01a\x01\xE3V[PPV[a\x02fa\x04WV[`\0a\x02q\x82a\x03PV[\x90P\x80Q`\0\x03a\x02\x9DW`@Qc4\x0CG=`\xE1\x1B\x81R`\x04\x81\x01\x83\x90R`$\x01[`@Q\x80\x91\x03\x90\xFD[`@Q\x7F\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\x02\x90a\x02\xCD\x90\x83\x90a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 \x80T`\xFF\x19\x16\x90Ua\x02\xF9`\0\x80Q` a\x08\xE3\x839\x81Q\x91R\x90V[`\0\x83\x81R`\x01\x91\x90\x91\x01` R`@\x81 a\x03\x14\x91a\x05}V[\x81\x7F\xCF\x95\xBB\xFEo\x87\x0F\x8C\xC4\x04\x82\xDC=\xCC\xDA\xFD&\x8F\x0E\x9C\xE0\xA4\xF2N\xA1\xBE\xA9\xBEd\xE5\x05\xFF\x82`@Qa\x03D\x91\x90a\x07!V[`@Q\x80\x91\x03\x90\xA2PPV[`\0\x81\x81R\x7F\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\x01` R`@\x90 \x80T``\x91\x90a\x03\x8C\x90a\x07pV[\x80`\x1F\x01` \x80\x91\x04\x02` \x01`@Q\x90\x81\x01`@R\x80\x92\x91\x90\x81\x81R` \x01\x82\x80Ta\x03\xB8\x90a\x07pV[\x80\x15a\x04\x05W\x80`\x1F\x10a\x03\xDAWa\x01\0\x80\x83T\x04\x02\x83R\x91` \x01\x91a\x04\x05V[\x82\x01\x91\x90`\0R` `\0 \x90[\x81T\x81R\x90`\x01\x01\x90` \x01\x80\x83\x11a\x03\xE8W\x82\x90\x03`\x1F\x16\x82\x01\x91[PPPPP\x90P\x91\x90PV[`\0`\0\x80Q` a\x08\xE3\x839\x81Q\x91R`@\x80Q`\x01`\x01`\xA0\x1B\x03\x85\x16` \x82\x01R`\x02\x92\x90\x92\x01\x91\x01`@\x80Q`\x1F\x19\x81\x84\x03\x01\x81R\x90\x82\x90Ra\x02\x1C\x91a\x07TV[a\x04`3a\x04\x11V[\x80a\x04jWP30\x14[\x15a\x04qWV[`@Qb\x82\xB4)`\xE8\x1B\x81R`\x04\x01`@Q\x80\x91\x03\x90\xFD[a\x01\xF7\x81`\0\x80Q` a\x08\xE3\x839\x81Q\x91R\x80T\x90`\0a\x04\xAA\x83a\x07\xAAV[\x91\x90PUa\x04\xB7\x82a\x01\xFAV[\x15a\x04\xD7W\x81`@QcF\x8B\x12\xAD`\xE1\x1B\x81R`\x04\x01a\x02\x94\x91\x90a\x07!V[`\x01`\0\x80Q` a\x08\xE3\x839\x81Q\x91R`\x02\x01\x83`@Qa\x04\xF9\x91\x90a\x07TV[\x90\x81R`@Q\x90\x81\x90\x03` \x01\x90 \x80T\x91\x15\x15`\xFF\x19\x90\x92\x16\x91\x90\x91\x17\x90U\x81a\x05/`\0\x80Q` a\x08\xE3\x839\x81Q\x91R\x90V[`\0\x83\x81R`\x01\x91\x90\x91\x01` R`@\x90 \x90a\x05L\x90\x82a\x08\"V[P\x80\x7F8\x10\x9E\xDC&\xE1f\xB5W\x93R\xCEV\xA5\x08\x13\x17~\xB2R\x08\xFD\x90\xD6\x1F/7\x83\x86\"\x02 \x83`@Qa\x03D\x91\x90a\x07!V[P\x80Ta\x05\x89\x90a\x07pV[`\0\x82U\x80`\x1F\x10a\x05\x99WPPV[`\x1F\x01` \x90\x04\x90`\0R` `\0 \x90\x81\x01\x90a\x01\xF7\x91\x90[\x80\x82\x11\x15a\x05\xC7W`\0\x81U`\x01\x01a\x05\xB3V[P\x90V[`\0\x80`@\x83\x85\x03\x12\x15a\x05\xDEW`\0\x80\xFD[PP\x805\x92` \x90\x91\x015\x91PV[`\0` \x82\x84\x03\x12\x15a\x05\xFFW`\0\x80\xFD[\x815`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x06\x16W`\0\x80\xFD[\x93\x92PPPV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`\0` \x82\x84\x03\x12\x15a\x06EW`\0\x80\xFD[\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x06]W`\0\x80\xFD[\x81\x84\x01\x91P\x84`\x1F\x83\x01\x12a\x06qW`\0\x80\xFD[\x815\x81\x81\x11\x15a\x06\x83Wa\x06\x83a\x06\x1DV[`@Q`\x1F\x82\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x83\x82\x11\x81\x83\x10\x17\x15a\x06\xABWa\x06\xABa\x06\x1DV[\x81`@R\x82\x81R\x87` \x84\x87\x01\x01\x11\x15a\x06\xC4W`\0\x80\xFD[\x82` \x86\x01` \x83\x017`\0\x92\x81\x01` \x01\x92\x90\x92RP\x95\x94PPPPPV[`\0` \x82\x84\x03\x12\x15a\x06\xF6W`\0\x80\xFD[P5\x91\x90PV[`\0[\x83\x81\x10\x15a\x07\x18W\x81\x81\x01Q\x83\x82\x01R` \x01a\x07\0V[PP`\0\x91\x01RV[` \x81R`\0\x82Q\x80` \x84\x01Ra\x07@\x81`@\x85\x01` \x87\x01a\x06\xFDV[`\x1F\x01`\x1F\x19\x16\x91\x90\x91\x01`@\x01\x92\x91PPV[`\0\x82Qa\x07f\x81\x84` \x87\x01a\x06\xFDV[\x91\x90\x91\x01\x92\x91PPV[`\x01\x81\x81\x1C\x90\x82\x16\x80a\x07\x84W`\x7F\x82\x16\x91P[` \x82\x10\x81\x03a\x07\xA4WcNH{q`\xE0\x1B`\0R`\"`\x04R`$`\0\xFD[P\x91\x90PV[`\0`\x01\x82\x01a\x07\xCAWcNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[P`\x01\x01\x90V[`\x1F\x82\x11\x15a\x08\x1DW`\0\x81`\0R` `\0 `\x1F\x85\x01`\x05\x1C\x81\x01` \x86\x10\x15a\x07\xFAWP\x80[`\x1F\x85\x01`\x05\x1C\x82\x01\x91P[\x81\x81\x10\x15a\x08\x19W\x82\x81U`\x01\x01a\x08\x06V[PPP[PPPV[\x81Qg\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x15a\x08<Wa\x08<a\x06\x1DV[a\x08P\x81a\x08J\x84Ta\x07pV[\x84a\x07\xD1V[` \x80`\x1F\x83\x11`\x01\x81\x14a\x08\x85W`\0\x84\x15a\x08mWP\x85\x83\x01Q[`\0\x19`\x03\x86\x90\x1B\x1C\x19\x16`\x01\x85\x90\x1B\x17\x85Ua\x08\x19V[`\0\x85\x81R` \x81 `\x1F\x19\x86\x16\x91[\x82\x81\x10\x15a\x08\xB4W\x88\x86\x01Q\x82U\x94\x84\x01\x94`\x01\x90\x91\x01\x90\x84\x01a\x08\x95V[P\x85\x82\x10\x15a\x08\xD2W\x87\x85\x01Q`\0\x19`\x03\x88\x90\x1B`\xF8\x16\x1C\x19\x16\x81U[PPPPP`\x01\x90\x81\x1B\x01\x90UPV\xFE\x97\xE2\xC6\xAA\xD4\xCE]V.\xBF\xAA\0\xDBk\x9E\x0F\xB6n\xA5\xD8\x16.\xD5\xB2C\xF5\x1A.\x03\x08o\0\xA2dipfsX\"\x12 \xBC\xE0j\x887K\x02\xFE<\x84\x04\x9C>I[gTmvF\xF0J\xDE\x10\xDD@\x17\xA7*\x8B2\xD7dsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static MULTIOWNABLE_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct MultiOwnable<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for MultiOwnable<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for MultiOwnable<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for MultiOwnable<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for MultiOwnable<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(MultiOwnable))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> MultiOwnable<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                MULTIOWNABLE_ABI.clone(),
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
                MULTIOWNABLE_ABI.clone(),
                MULTIOWNABLE_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `addOwnerAddress` (0x0f0f3f24) function
        pub fn add_owner_address(
            &self,
            owner: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([15, 15, 63, 36], owner)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `addOwnerPublicKey` (0x29565e3b) function
        pub fn add_owner_public_key(
            &self,
            x: [u8; 32],
            y: [u8; 32],
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([41, 86, 94, 59], (x, y))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `isOwnerAddress` (0xa2e1a8d8) function
        pub fn is_owner_address(
            &self,
            account: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, bool> {
            self.0
                .method_hash([162, 225, 168, 216], account)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `isOwnerBytes` (0x1ca5393f) function
        pub fn is_owner_bytes(
            &self,
            account: ::ethers::core::types::Bytes,
        ) -> ::ethers::contract::builders::ContractCall<M, bool> {
            self.0
                .method_hash([28, 165, 57, 63], account)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `isOwnerPublicKey` (0x066a1eb7) function
        pub fn is_owner_public_key(
            &self,
            x: [u8; 32],
            y: [u8; 32],
        ) -> ::ethers::contract::builders::ContractCall<M, bool> {
            self.0
                .method_hash([6, 106, 30, 183], (x, y))
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `nextOwnerIndex` (0xd948fd2e) function
        pub fn next_owner_index(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([217, 72, 253, 46], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `ownerAtIndex` (0x8ea69029) function
        pub fn owner_at_index(
            &self,
            index: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Bytes> {
            self.0
                .method_hash([142, 166, 144, 41], index)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `removeOwnerAtIndex` (0x72de3b5a) function
        pub fn remove_owner_at_index(
            &self,
            index: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([114, 222, 59, 90], index)
                .expect("method not found (this should never happen)")
        }
        ///Gets the contract's `AddOwner` event
        pub fn add_owner_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, AddOwnerFilter> {
            self.0.event()
        }
        ///Gets the contract's `RemoveOwner` event
        pub fn remove_owner_filter(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, RemoveOwnerFilter>
        {
            self.0.event()
        }
        /// Returns an `Event` builder for all the events of this contract.
        pub fn events(
            &self,
        ) -> ::ethers::contract::builders::Event<::std::sync::Arc<M>, M, MultiOwnableEvents>
        {
            self.0
                .event_with_filter(::core::default::Default::default())
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>> for MultiOwnable<M> {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `AlreadyOwner` with signature `AlreadyOwner(bytes)` and selector `0x8d16255a`
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
    #[etherror(name = "AlreadyOwner", abi = "AlreadyOwner(bytes)")]
    pub struct AlreadyOwner {
        pub owner: ::ethers::core::types::Bytes,
    }
    ///Custom Error type `InvalidEthereumAddressOwner` with signature `InvalidEthereumAddressOwner(bytes)` and selector `0xbff1ac65`
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
        name = "InvalidEthereumAddressOwner",
        abi = "InvalidEthereumAddressOwner(bytes)"
    )]
    pub struct InvalidEthereumAddressOwner {
        pub owner: ::ethers::core::types::Bytes,
    }
    ///Custom Error type `InvalidOwnerBytesLength` with signature `InvalidOwnerBytesLength(bytes)` and selector `0x4eeab722`
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
        name = "InvalidOwnerBytesLength",
        abi = "InvalidOwnerBytesLength(bytes)"
    )]
    pub struct InvalidOwnerBytesLength {
        pub owner: ::ethers::core::types::Bytes,
    }
    ///Custom Error type `NoOwnerAtIndex` with signature `NoOwnerAtIndex(uint256)` and selector `0x68188e7a`
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
    #[etherror(name = "NoOwnerAtIndex", abi = "NoOwnerAtIndex(uint256)")]
    pub struct NoOwnerAtIndex {
        pub index: ::ethers::core::types::U256,
    }
    ///Custom Error type `Unauthorized` with signature `Unauthorized()` and selector `0x82b42900`
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
    #[etherror(name = "Unauthorized", abi = "Unauthorized()")]
    pub struct Unauthorized;
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
    pub enum MultiOwnableErrors {
        AlreadyOwner(AlreadyOwner),
        InvalidEthereumAddressOwner(InvalidEthereumAddressOwner),
        InvalidOwnerBytesLength(InvalidOwnerBytesLength),
        NoOwnerAtIndex(NoOwnerAtIndex),
        Unauthorized(Unauthorized),
        /// The standard solidity revert string, with selector
        /// Error(string) -- 0x08c379a0
        RevertString(::std::string::String),
    }
    impl ::ethers::core::abi::AbiDecode for MultiOwnableErrors {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <::std::string::String as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RevertString(decoded));
            }
            if let Ok(decoded) = <AlreadyOwner as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::AlreadyOwner(decoded));
            }
            if let Ok(decoded) =
                <InvalidEthereumAddressOwner as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::InvalidEthereumAddressOwner(decoded));
            }
            if let Ok(decoded) =
                <InvalidOwnerBytesLength as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::InvalidOwnerBytesLength(decoded));
            }
            if let Ok(decoded) = <NoOwnerAtIndex as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::NoOwnerAtIndex(decoded));
            }
            if let Ok(decoded) = <Unauthorized as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::Unauthorized(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for MultiOwnableErrors {
        fn encode(self) -> ::std::vec::Vec<u8> {
            match self {
                Self::AlreadyOwner(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::InvalidEthereumAddressOwner(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::InvalidOwnerBytesLength(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::NoOwnerAtIndex(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::Unauthorized(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RevertString(s) => ::ethers::core::abi::AbiEncode::encode(s),
            }
        }
    }
    impl ::ethers::contract::ContractRevert for MultiOwnableErrors {
        fn valid_selector(selector: [u8; 4]) -> bool {
            match selector {
                [0x08, 0xc3, 0x79, 0xa0] => true,
                _ if selector == <AlreadyOwner as ::ethers::contract::EthError>::selector() => true,
                _ if selector
                    == <InvalidEthereumAddressOwner as ::ethers::contract::EthError>::selector(
                    ) =>
                {
                    true
                }
                _ if selector
                    == <InvalidOwnerBytesLength as ::ethers::contract::EthError>::selector() =>
                {
                    true
                }
                _ if selector == <NoOwnerAtIndex as ::ethers::contract::EthError>::selector() => {
                    true
                }
                _ if selector == <Unauthorized as ::ethers::contract::EthError>::selector() => true,
                _ => false,
            }
        }
    }
    impl ::core::fmt::Display for MultiOwnableErrors {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AlreadyOwner(element) => ::core::fmt::Display::fmt(element, f),
                Self::InvalidEthereumAddressOwner(element) => ::core::fmt::Display::fmt(element, f),
                Self::InvalidOwnerBytesLength(element) => ::core::fmt::Display::fmt(element, f),
                Self::NoOwnerAtIndex(element) => ::core::fmt::Display::fmt(element, f),
                Self::Unauthorized(element) => ::core::fmt::Display::fmt(element, f),
                Self::RevertString(s) => ::core::fmt::Display::fmt(s, f),
            }
        }
    }
    impl ::core::convert::From<::std::string::String> for MultiOwnableErrors {
        fn from(value: String) -> Self {
            Self::RevertString(value)
        }
    }
    impl ::core::convert::From<AlreadyOwner> for MultiOwnableErrors {
        fn from(value: AlreadyOwner) -> Self {
            Self::AlreadyOwner(value)
        }
    }
    impl ::core::convert::From<InvalidEthereumAddressOwner> for MultiOwnableErrors {
        fn from(value: InvalidEthereumAddressOwner) -> Self {
            Self::InvalidEthereumAddressOwner(value)
        }
    }
    impl ::core::convert::From<InvalidOwnerBytesLength> for MultiOwnableErrors {
        fn from(value: InvalidOwnerBytesLength) -> Self {
            Self::InvalidOwnerBytesLength(value)
        }
    }
    impl ::core::convert::From<NoOwnerAtIndex> for MultiOwnableErrors {
        fn from(value: NoOwnerAtIndex) -> Self {
            Self::NoOwnerAtIndex(value)
        }
    }
    impl ::core::convert::From<Unauthorized> for MultiOwnableErrors {
        fn from(value: Unauthorized) -> Self {
            Self::Unauthorized(value)
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
    #[ethevent(name = "AddOwner", abi = "AddOwner(uint256,bytes)")]
    pub struct AddOwnerFilter {
        #[ethevent(indexed)]
        pub index: ::ethers::core::types::U256,
        pub owner: ::ethers::core::types::Bytes,
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
    #[ethevent(name = "RemoveOwner", abi = "RemoveOwner(uint256,bytes)")]
    pub struct RemoveOwnerFilter {
        #[ethevent(indexed)]
        pub index: ::ethers::core::types::U256,
        pub owner: ::ethers::core::types::Bytes,
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
    pub enum MultiOwnableEvents {
        AddOwnerFilter(AddOwnerFilter),
        RemoveOwnerFilter(RemoveOwnerFilter),
    }
    impl ::ethers::contract::EthLogDecode for MultiOwnableEvents {
        fn decode_log(
            log: &::ethers::core::abi::RawLog,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::Error> {
            if let Ok(decoded) = AddOwnerFilter::decode_log(log) {
                return Ok(MultiOwnableEvents::AddOwnerFilter(decoded));
            }
            if let Ok(decoded) = RemoveOwnerFilter::decode_log(log) {
                return Ok(MultiOwnableEvents::RemoveOwnerFilter(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData)
        }
    }
    impl ::core::fmt::Display for MultiOwnableEvents {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AddOwnerFilter(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveOwnerFilter(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AddOwnerFilter> for MultiOwnableEvents {
        fn from(value: AddOwnerFilter) -> Self {
            Self::AddOwnerFilter(value)
        }
    }
    impl ::core::convert::From<RemoveOwnerFilter> for MultiOwnableEvents {
        fn from(value: RemoveOwnerFilter) -> Self {
            Self::RemoveOwnerFilter(value)
        }
    }
    ///Container type for all input parameters for the `addOwnerAddress` function with signature `addOwnerAddress(address)` and selector `0x0f0f3f24`
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
    #[ethcall(name = "addOwnerAddress", abi = "addOwnerAddress(address)")]
    pub struct AddOwnerAddressCall {
        pub owner: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `addOwnerPublicKey` function with signature `addOwnerPublicKey(bytes32,bytes32)` and selector `0x29565e3b`
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
    #[ethcall(name = "addOwnerPublicKey", abi = "addOwnerPublicKey(bytes32,bytes32)")]
    pub struct AddOwnerPublicKeyCall {
        pub x: [u8; 32],
        pub y: [u8; 32],
    }
    ///Container type for all input parameters for the `isOwnerAddress` function with signature `isOwnerAddress(address)` and selector `0xa2e1a8d8`
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
    #[ethcall(name = "isOwnerAddress", abi = "isOwnerAddress(address)")]
    pub struct IsOwnerAddressCall {
        pub account: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `isOwnerBytes` function with signature `isOwnerBytes(bytes)` and selector `0x1ca5393f`
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
    #[ethcall(name = "isOwnerBytes", abi = "isOwnerBytes(bytes)")]
    pub struct IsOwnerBytesCall {
        pub account: ::ethers::core::types::Bytes,
    }
    ///Container type for all input parameters for the `isOwnerPublicKey` function with signature `isOwnerPublicKey(bytes32,bytes32)` and selector `0x066a1eb7`
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
    #[ethcall(name = "isOwnerPublicKey", abi = "isOwnerPublicKey(bytes32,bytes32)")]
    pub struct IsOwnerPublicKeyCall {
        pub x: [u8; 32],
        pub y: [u8; 32],
    }
    ///Container type for all input parameters for the `nextOwnerIndex` function with signature `nextOwnerIndex()` and selector `0xd948fd2e`
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
    #[ethcall(name = "nextOwnerIndex", abi = "nextOwnerIndex()")]
    pub struct NextOwnerIndexCall;
    ///Container type for all input parameters for the `ownerAtIndex` function with signature `ownerAtIndex(uint256)` and selector `0x8ea69029`
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
    #[ethcall(name = "ownerAtIndex", abi = "ownerAtIndex(uint256)")]
    pub struct OwnerAtIndexCall {
        pub index: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `removeOwnerAtIndex` function with signature `removeOwnerAtIndex(uint256)` and selector `0x72de3b5a`
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
    #[ethcall(name = "removeOwnerAtIndex", abi = "removeOwnerAtIndex(uint256)")]
    pub struct RemoveOwnerAtIndexCall {
        pub index: ::ethers::core::types::U256,
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
    pub enum MultiOwnableCalls {
        AddOwnerAddress(AddOwnerAddressCall),
        AddOwnerPublicKey(AddOwnerPublicKeyCall),
        IsOwnerAddress(IsOwnerAddressCall),
        IsOwnerBytes(IsOwnerBytesCall),
        IsOwnerPublicKey(IsOwnerPublicKeyCall),
        NextOwnerIndex(NextOwnerIndexCall),
        OwnerAtIndex(OwnerAtIndexCall),
        RemoveOwnerAtIndex(RemoveOwnerAtIndexCall),
    }
    impl ::ethers::core::abi::AbiDecode for MultiOwnableCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) =
                <AddOwnerAddressCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AddOwnerAddress(decoded));
            }
            if let Ok(decoded) =
                <AddOwnerPublicKeyCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::AddOwnerPublicKey(decoded));
            }
            if let Ok(decoded) =
                <IsOwnerAddressCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IsOwnerAddress(decoded));
            }
            if let Ok(decoded) = <IsOwnerBytesCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IsOwnerBytes(decoded));
            }
            if let Ok(decoded) =
                <IsOwnerPublicKeyCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::IsOwnerPublicKey(decoded));
            }
            if let Ok(decoded) =
                <NextOwnerIndexCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::NextOwnerIndex(decoded));
            }
            if let Ok(decoded) = <OwnerAtIndexCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::OwnerAtIndex(decoded));
            }
            if let Ok(decoded) =
                <RemoveOwnerAtIndexCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::RemoveOwnerAtIndex(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for MultiOwnableCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::AddOwnerAddress(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::AddOwnerPublicKey(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::IsOwnerAddress(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::IsOwnerBytes(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::IsOwnerPublicKey(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::NextOwnerIndex(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::OwnerAtIndex(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::RemoveOwnerAtIndex(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
            }
        }
    }
    impl ::core::fmt::Display for MultiOwnableCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::AddOwnerAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::AddOwnerPublicKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::IsOwnerAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::IsOwnerBytes(element) => ::core::fmt::Display::fmt(element, f),
                Self::IsOwnerPublicKey(element) => ::core::fmt::Display::fmt(element, f),
                Self::NextOwnerIndex(element) => ::core::fmt::Display::fmt(element, f),
                Self::OwnerAtIndex(element) => ::core::fmt::Display::fmt(element, f),
                Self::RemoveOwnerAtIndex(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<AddOwnerAddressCall> for MultiOwnableCalls {
        fn from(value: AddOwnerAddressCall) -> Self {
            Self::AddOwnerAddress(value)
        }
    }
    impl ::core::convert::From<AddOwnerPublicKeyCall> for MultiOwnableCalls {
        fn from(value: AddOwnerPublicKeyCall) -> Self {
            Self::AddOwnerPublicKey(value)
        }
    }
    impl ::core::convert::From<IsOwnerAddressCall> for MultiOwnableCalls {
        fn from(value: IsOwnerAddressCall) -> Self {
            Self::IsOwnerAddress(value)
        }
    }
    impl ::core::convert::From<IsOwnerBytesCall> for MultiOwnableCalls {
        fn from(value: IsOwnerBytesCall) -> Self {
            Self::IsOwnerBytes(value)
        }
    }
    impl ::core::convert::From<IsOwnerPublicKeyCall> for MultiOwnableCalls {
        fn from(value: IsOwnerPublicKeyCall) -> Self {
            Self::IsOwnerPublicKey(value)
        }
    }
    impl ::core::convert::From<NextOwnerIndexCall> for MultiOwnableCalls {
        fn from(value: NextOwnerIndexCall) -> Self {
            Self::NextOwnerIndex(value)
        }
    }
    impl ::core::convert::From<OwnerAtIndexCall> for MultiOwnableCalls {
        fn from(value: OwnerAtIndexCall) -> Self {
            Self::OwnerAtIndex(value)
        }
    }
    impl ::core::convert::From<RemoveOwnerAtIndexCall> for MultiOwnableCalls {
        fn from(value: RemoveOwnerAtIndexCall) -> Self {
            Self::RemoveOwnerAtIndex(value)
        }
    }
    ///Container type for all return fields from the `isOwnerAddress` function with signature `isOwnerAddress(address)` and selector `0xa2e1a8d8`
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
    pub struct IsOwnerAddressReturn(pub bool);
    ///Container type for all return fields from the `isOwnerBytes` function with signature `isOwnerBytes(bytes)` and selector `0x1ca5393f`
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
    pub struct IsOwnerBytesReturn(pub bool);
    ///Container type for all return fields from the `isOwnerPublicKey` function with signature `isOwnerPublicKey(bytes32,bytes32)` and selector `0x066a1eb7`
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
    pub struct IsOwnerPublicKeyReturn(pub bool);
    ///Container type for all return fields from the `nextOwnerIndex` function with signature `nextOwnerIndex()` and selector `0xd948fd2e`
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
    pub struct NextOwnerIndexReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `ownerAtIndex` function with signature `ownerAtIndex(uint256)` and selector `0x8ea69029`
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
    pub struct OwnerAtIndexReturn(pub ::ethers::core::types::Bytes);
}
