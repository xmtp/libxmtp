pub use mock_entry_point::*;
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
pub mod mock_entry_point {
    pub use super::super::shared_types::*;
    #[allow(deprecated)]
    fn __abi() -> ::ethers::core::abi::Abi {
        ::ethers::core::abi::ethabi::Contract {
            constructor: ::core::option::Option::None,
            functions: ::core::convert::From::from([
                (
                    ::std::borrow::ToOwned::to_owned("balanceOf"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("balanceOf"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::string::String::new(),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
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
                    ::std::borrow::ToOwned::to_owned("depositTo"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("depositTo"),
                        inputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("to"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Address,
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("address"),
                            ),
                        },],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("validateUserOp"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("validateUserOp"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("account"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("userOp"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Tuple(::std::vec![
                                    ::ethers::core::abi::ethabi::ParamType::Address,
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::Uint(256usize),
                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                    ::ethers::core::abi::ethabi::ParamType::Bytes,
                                ],),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("struct UserOperation"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("userOpHash"),
                                kind: ::ethers::core::abi::ethabi::ParamType::FixedBytes(32usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("bytes32"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("missingAccountFunds",),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![::ethers::core::abi::ethabi::Param {
                            name: ::std::borrow::ToOwned::to_owned("validationData"),
                            kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                            internal_type: ::core::option::Option::Some(
                                ::std::borrow::ToOwned::to_owned("uint256"),
                            ),
                        },],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
                (
                    ::std::borrow::ToOwned::to_owned("withdrawTo"),
                    ::std::vec![::ethers::core::abi::ethabi::Function {
                        name: ::std::borrow::ToOwned::to_owned("withdrawTo"),
                        inputs: ::std::vec![
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("to"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Address,
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("address"),
                                ),
                            },
                            ::ethers::core::abi::ethabi::Param {
                                name: ::std::borrow::ToOwned::to_owned("amount"),
                                kind: ::ethers::core::abi::ethabi::ParamType::Uint(256usize,),
                                internal_type: ::core::option::Option::Some(
                                    ::std::borrow::ToOwned::to_owned("uint256"),
                                ),
                            },
                        ],
                        outputs: ::std::vec![],
                        constant: ::core::option::Option::None,
                        state_mutability: ::ethers::core::abi::ethabi::StateMutability::Payable,
                    },],
                ),
            ]),
            events: ::std::collections::BTreeMap::new(),
            errors: ::std::collections::BTreeMap::new(),
            receive: true,
            fallback: false,
        }
    }
    ///The parsed JSON ABI of the contract.
    pub static MOCKENTRYPOINT_ABI: ::ethers::contract::Lazy<::ethers::core::abi::Abi> =
        ::ethers::contract::Lazy::new(__abi);
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = b"`\x80`@R4\x80\x15a\0\x10W`\0\x80\xFD[Pa\x06O\x80a\0 `\09`\0\xF3\xFE`\x80`@R`\x046\x10a\0CW`\x005`\xE0\x1C\x80c \\(x\x14a\0XW\x80cp\xA0\x821\x14a\0kW\x80c\xA1\x9D\x19\xD5\x14a\0\xAAW\x80c\xB7`\xFA\xF9\x14a\0\xBDW`\0\x80\xFD[6a\0SWa\0Q3a\0\xCBV[\0[`\0\x80\xFD[a\0Qa\0f6`\x04a\x02\x1DV[a\0\xFBV[4\x80\x15a\0wW`\0\x80\xFD[Pa\0\x98a\0\x866`\x04a\x02GV[`\0` \x81\x90R\x90\x81R`@\x90 T\x81V[`@Q\x90\x81R` \x01`@Q\x80\x91\x03\x90\xF3[a\0\x98a\0\xB86`\x04a\x036V[a\x01\x82V[a\0Qa\0\xCB6`\x04a\x02GV[`\x01`\x01`\xA0\x1B\x03\x81\x16`\0\x90\x81R` \x81\x90R`@\x81 \x80T4\x92\x90a\0\xF3\x90\x84\x90a\x04\x9CV[\x90\x91UPPPV[3`\0\x90\x81R` \x81\x90R`@\x81 \x80T\x83\x92\x90a\x01\x1A\x90\x84\x90a\x04\xB5V[\x90\x91UPP`@Q`\0\x90`\x01`\x01`\xA0\x1B\x03\x84\x16\x90\x83\x90\x83\x81\x81\x81\x85\x87Z\xF1\x92PPP=\x80`\0\x81\x14a\x01jW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=`\0` \x84\x01>a\x01oV[``\x91P[PP\x90P\x80a\x01}W`\0\x80\xFD[PPPV[`@Qc:\x87\x1C\xDD`\xE0\x1B\x81R`\0\x90`\x01`\x01`\xA0\x1B\x03\x86\x16\x90c:\x87\x1C\xDD\x90a\x01\xB5\x90\x87\x90\x87\x90\x87\x90`\x04\x01a\x05\x0EV[` `@Q\x80\x83\x03\x81`\0\x87Z\xF1\x15\x80\x15a\x01\xD4W=`\0\x80>=`\0\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x01\xF8\x91\x90a\x06\0V[\x95\x94PPPPPV[\x805`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x02\x18W`\0\x80\xFD[\x91\x90PV[`\0\x80`@\x83\x85\x03\x12\x15a\x020W`\0\x80\xFD[a\x029\x83a\x02\x01V[\x94` \x93\x90\x93\x015\x93PPPV[`\0` \x82\x84\x03\x12\x15a\x02YW`\0\x80\xFD[a\x02b\x82a\x02\x01V[\x93\x92PPPV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`@Qa\x01`\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x02\xA3Wa\x02\xA3a\x02iV[`@R\x90V[`\0\x82`\x1F\x83\x01\x12a\x02\xBAW`\0\x80\xFD[\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x02\xD5Wa\x02\xD5a\x02iV[`@Q`\x1F\x83\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x82\x82\x11\x81\x83\x10\x17\x15a\x02\xFDWa\x02\xFDa\x02iV[\x81`@R\x83\x81R\x86` \x85\x88\x01\x01\x11\x15a\x03\x16W`\0\x80\xFD[\x83` \x87\x01` \x83\x017`\0` \x85\x83\x01\x01R\x80\x94PPPPP\x92\x91PPV[`\0\x80`\0\x80`\x80\x85\x87\x03\x12\x15a\x03LW`\0\x80\xFD[a\x03U\x85a\x02\x01V[\x93P` \x85\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x03rW`\0\x80\xFD[\x90\x86\x01\x90a\x01`\x82\x89\x03\x12\x15a\x03\x87W`\0\x80\xFD[a\x03\x8Fa\x02\x7FV[a\x03\x98\x83a\x02\x01V[\x81R` \x83\x015` \x82\x01R`@\x83\x015\x82\x81\x11\x15a\x03\xB6W`\0\x80\xFD[a\x03\xC2\x8A\x82\x86\x01a\x02\xA9V[`@\x83\x01RP``\x83\x015\x82\x81\x11\x15a\x03\xDAW`\0\x80\xFD[a\x03\xE6\x8A\x82\x86\x01a\x02\xA9V[``\x83\x01RP`\x80\x83\x015`\x80\x82\x01R`\xA0\x83\x015`\xA0\x82\x01R`\xC0\x83\x015`\xC0\x82\x01R`\xE0\x83\x015`\xE0\x82\x01Ra\x01\0\x80\x84\x015\x81\x83\x01RPa\x01 \x80\x84\x015\x83\x81\x11\x15a\x044W`\0\x80\xFD[a\x04@\x8B\x82\x87\x01a\x02\xA9V[\x82\x84\x01RPPa\x01@\x80\x84\x015\x83\x81\x11\x15a\x04ZW`\0\x80\xFD[a\x04f\x8B\x82\x87\x01a\x02\xA9V[\x91\x83\x01\x91\x90\x91RP\x95\x98\x95\x97PPPP`@\x84\x015\x93``\x015\x92\x91PPV[cNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[\x80\x82\x01\x80\x82\x11\x15a\x04\xAFWa\x04\xAFa\x04\x86V[\x92\x91PPV[\x81\x81\x03\x81\x81\x11\x15a\x04\xAFWa\x04\xAFa\x04\x86V[`\0\x81Q\x80\x84R`\0[\x81\x81\x10\x15a\x04\xEEW` \x81\x85\x01\x81\x01Q\x86\x83\x01\x82\x01R\x01a\x04\xD2V[P`\0` \x82\x86\x01\x01R` `\x1F\x19`\x1F\x83\x01\x16\x85\x01\x01\x91PP\x92\x91PPV[``\x81Ra\x05(``\x82\x01\x85Q`\x01`\x01`\xA0\x1B\x03\x16\x90RV[` \x84\x01Q`\x80\x82\x01R`\0`@\x85\x01Qa\x01`\x80`\xA0\x85\x01Ra\x05Pa\x01\xC0\x85\x01\x83a\x04\xC8V[\x91P``\x87\x01Q`_\x19\x80\x86\x85\x03\x01`\xC0\x87\x01Ra\x05n\x84\x83a\x04\xC8V[\x93P`\x80\x89\x01Q`\xE0\x87\x01R`\xA0\x89\x01Q\x91Pa\x01\0\x82\x81\x88\x01R`\xC0\x8A\x01Q\x92Pa\x01 \x83\x81\x89\x01R`\xE0\x8B\x01Q\x93Pa\x01@\x84\x81\x8A\x01R\x82\x8C\x01Q\x86\x8A\x01R\x81\x8C\x01Q\x95P\x83\x89\x88\x03\x01a\x01\x80\x8A\x01Ra\x05\xCA\x87\x87a\x04\xC8V[\x96P\x80\x8C\x01Q\x95PPPP\x80\x86\x85\x03\x01a\x01\xA0\x87\x01RPPa\x05\xEC\x82\x82a\x04\xC8V[` \x85\x01\x96\x90\x96RPPP`@\x01R\x91\x90PV[`\0` \x82\x84\x03\x12\x15a\x06\x12W`\0\x80\xFD[PQ\x91\x90PV\xFE\xA2dipfsX\"\x12 \xF4:J\xCD\xE6\xE3\xF3\xAF\xC1\xCC\xFA\xE5\xEF~?\x1B$9n\x8FV\x92b\x02\xC8\xBC\xF9\xD3\xD8|\xAA\xBCdsolcC\0\x08\x17\x003";
    /// The bytecode of the contract.
    pub static MOCKENTRYPOINT_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = b"`\x80`@R`\x046\x10a\0CW`\x005`\xE0\x1C\x80c \\(x\x14a\0XW\x80cp\xA0\x821\x14a\0kW\x80c\xA1\x9D\x19\xD5\x14a\0\xAAW\x80c\xB7`\xFA\xF9\x14a\0\xBDW`\0\x80\xFD[6a\0SWa\0Q3a\0\xCBV[\0[`\0\x80\xFD[a\0Qa\0f6`\x04a\x02\x1DV[a\0\xFBV[4\x80\x15a\0wW`\0\x80\xFD[Pa\0\x98a\0\x866`\x04a\x02GV[`\0` \x81\x90R\x90\x81R`@\x90 T\x81V[`@Q\x90\x81R` \x01`@Q\x80\x91\x03\x90\xF3[a\0\x98a\0\xB86`\x04a\x036V[a\x01\x82V[a\0Qa\0\xCB6`\x04a\x02GV[`\x01`\x01`\xA0\x1B\x03\x81\x16`\0\x90\x81R` \x81\x90R`@\x81 \x80T4\x92\x90a\0\xF3\x90\x84\x90a\x04\x9CV[\x90\x91UPPPV[3`\0\x90\x81R` \x81\x90R`@\x81 \x80T\x83\x92\x90a\x01\x1A\x90\x84\x90a\x04\xB5V[\x90\x91UPP`@Q`\0\x90`\x01`\x01`\xA0\x1B\x03\x84\x16\x90\x83\x90\x83\x81\x81\x81\x85\x87Z\xF1\x92PPP=\x80`\0\x81\x14a\x01jW`@Q\x91P`\x1F\x19`?=\x01\x16\x82\x01`@R=\x82R=`\0` \x84\x01>a\x01oV[``\x91P[PP\x90P\x80a\x01}W`\0\x80\xFD[PPPV[`@Qc:\x87\x1C\xDD`\xE0\x1B\x81R`\0\x90`\x01`\x01`\xA0\x1B\x03\x86\x16\x90c:\x87\x1C\xDD\x90a\x01\xB5\x90\x87\x90\x87\x90\x87\x90`\x04\x01a\x05\x0EV[` `@Q\x80\x83\x03\x81`\0\x87Z\xF1\x15\x80\x15a\x01\xD4W=`\0\x80>=`\0\xFD[PPPP`@Q=`\x1F\x19`\x1F\x82\x01\x16\x82\x01\x80`@RP\x81\x01\x90a\x01\xF8\x91\x90a\x06\0V[\x95\x94PPPPPV[\x805`\x01`\x01`\xA0\x1B\x03\x81\x16\x81\x14a\x02\x18W`\0\x80\xFD[\x91\x90PV[`\0\x80`@\x83\x85\x03\x12\x15a\x020W`\0\x80\xFD[a\x029\x83a\x02\x01V[\x94` \x93\x90\x93\x015\x93PPPV[`\0` \x82\x84\x03\x12\x15a\x02YW`\0\x80\xFD[a\x02b\x82a\x02\x01V[\x93\x92PPPV[cNH{q`\xE0\x1B`\0R`A`\x04R`$`\0\xFD[`@Qa\x01`\x81\x01g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x81\x11\x82\x82\x10\x17\x15a\x02\xA3Wa\x02\xA3a\x02iV[`@R\x90V[`\0\x82`\x1F\x83\x01\x12a\x02\xBAW`\0\x80\xFD[\x815g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x02\xD5Wa\x02\xD5a\x02iV[`@Q`\x1F\x83\x01`\x1F\x19\x90\x81\x16`?\x01\x16\x81\x01\x90\x82\x82\x11\x81\x83\x10\x17\x15a\x02\xFDWa\x02\xFDa\x02iV[\x81`@R\x83\x81R\x86` \x85\x88\x01\x01\x11\x15a\x03\x16W`\0\x80\xFD[\x83` \x87\x01` \x83\x017`\0` \x85\x83\x01\x01R\x80\x94PPPPP\x92\x91PPV[`\0\x80`\0\x80`\x80\x85\x87\x03\x12\x15a\x03LW`\0\x80\xFD[a\x03U\x85a\x02\x01V[\x93P` \x85\x015g\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\x80\x82\x11\x15a\x03rW`\0\x80\xFD[\x90\x86\x01\x90a\x01`\x82\x89\x03\x12\x15a\x03\x87W`\0\x80\xFD[a\x03\x8Fa\x02\x7FV[a\x03\x98\x83a\x02\x01V[\x81R` \x83\x015` \x82\x01R`@\x83\x015\x82\x81\x11\x15a\x03\xB6W`\0\x80\xFD[a\x03\xC2\x8A\x82\x86\x01a\x02\xA9V[`@\x83\x01RP``\x83\x015\x82\x81\x11\x15a\x03\xDAW`\0\x80\xFD[a\x03\xE6\x8A\x82\x86\x01a\x02\xA9V[``\x83\x01RP`\x80\x83\x015`\x80\x82\x01R`\xA0\x83\x015`\xA0\x82\x01R`\xC0\x83\x015`\xC0\x82\x01R`\xE0\x83\x015`\xE0\x82\x01Ra\x01\0\x80\x84\x015\x81\x83\x01RPa\x01 \x80\x84\x015\x83\x81\x11\x15a\x044W`\0\x80\xFD[a\x04@\x8B\x82\x87\x01a\x02\xA9V[\x82\x84\x01RPPa\x01@\x80\x84\x015\x83\x81\x11\x15a\x04ZW`\0\x80\xFD[a\x04f\x8B\x82\x87\x01a\x02\xA9V[\x91\x83\x01\x91\x90\x91RP\x95\x98\x95\x97PPPP`@\x84\x015\x93``\x015\x92\x91PPV[cNH{q`\xE0\x1B`\0R`\x11`\x04R`$`\0\xFD[\x80\x82\x01\x80\x82\x11\x15a\x04\xAFWa\x04\xAFa\x04\x86V[\x92\x91PPV[\x81\x81\x03\x81\x81\x11\x15a\x04\xAFWa\x04\xAFa\x04\x86V[`\0\x81Q\x80\x84R`\0[\x81\x81\x10\x15a\x04\xEEW` \x81\x85\x01\x81\x01Q\x86\x83\x01\x82\x01R\x01a\x04\xD2V[P`\0` \x82\x86\x01\x01R` `\x1F\x19`\x1F\x83\x01\x16\x85\x01\x01\x91PP\x92\x91PPV[``\x81Ra\x05(``\x82\x01\x85Q`\x01`\x01`\xA0\x1B\x03\x16\x90RV[` \x84\x01Q`\x80\x82\x01R`\0`@\x85\x01Qa\x01`\x80`\xA0\x85\x01Ra\x05Pa\x01\xC0\x85\x01\x83a\x04\xC8V[\x91P``\x87\x01Q`_\x19\x80\x86\x85\x03\x01`\xC0\x87\x01Ra\x05n\x84\x83a\x04\xC8V[\x93P`\x80\x89\x01Q`\xE0\x87\x01R`\xA0\x89\x01Q\x91Pa\x01\0\x82\x81\x88\x01R`\xC0\x8A\x01Q\x92Pa\x01 \x83\x81\x89\x01R`\xE0\x8B\x01Q\x93Pa\x01@\x84\x81\x8A\x01R\x82\x8C\x01Q\x86\x8A\x01R\x81\x8C\x01Q\x95P\x83\x89\x88\x03\x01a\x01\x80\x8A\x01Ra\x05\xCA\x87\x87a\x04\xC8V[\x96P\x80\x8C\x01Q\x95PPPP\x80\x86\x85\x03\x01a\x01\xA0\x87\x01RPPa\x05\xEC\x82\x82a\x04\xC8V[` \x85\x01\x96\x90\x96RPPP`@\x01R\x91\x90PV[`\0` \x82\x84\x03\x12\x15a\x06\x12W`\0\x80\xFD[PQ\x91\x90PV\xFE\xA2dipfsX\"\x12 \xF4:J\xCD\xE6\xE3\xF3\xAF\xC1\xCC\xFA\xE5\xEF~?\x1B$9n\x8FV\x92b\x02\xC8\xBC\xF9\xD3\xD8|\xAA\xBCdsolcC\0\x08\x17\x003";
    /// The deployed bytecode of the contract.
    pub static MOCKENTRYPOINT_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct MockEntryPoint<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for MockEntryPoint<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for MockEntryPoint<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for MockEntryPoint<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for MockEntryPoint<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(::core::stringify!(MockEntryPoint))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> MockEntryPoint<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                MOCKENTRYPOINT_ABI.clone(),
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
                MOCKENTRYPOINT_ABI.clone(),
                MOCKENTRYPOINT_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `balanceOf` (0x70a08231) function
        pub fn balance_of(
            &self,
            p0: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([112, 160, 130, 49], p0)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `depositTo` (0xb760faf9) function
        pub fn deposit_to(
            &self,
            to: ::ethers::core::types::Address,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([183, 96, 250, 249], to)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `validateUserOp` (0xa19d19d5) function
        pub fn validate_user_op(
            &self,
            account: ::ethers::core::types::Address,
            user_op: UserOperation,
            user_op_hash: [u8; 32],
            missing_account_funds: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash(
                    [161, 157, 25, 213],
                    (account, user_op, user_op_hash, missing_account_funds),
                )
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `withdrawTo` (0x205c2878) function
        pub fn withdraw_to(
            &self,
            to: ::ethers::core::types::Address,
            amount: ::ethers::core::types::U256,
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([32, 92, 40, 120], (to, amount))
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for MockEntryPoint<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Container type for all input parameters for the `balanceOf` function with signature `balanceOf(address)` and selector `0x70a08231`
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
    #[ethcall(name = "balanceOf", abi = "balanceOf(address)")]
    pub struct BalanceOfCall(pub ::ethers::core::types::Address);
    ///Container type for all input parameters for the `depositTo` function with signature `depositTo(address)` and selector `0xb760faf9`
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
    #[ethcall(name = "depositTo", abi = "depositTo(address)")]
    pub struct DepositToCall {
        pub to: ::ethers::core::types::Address,
    }
    ///Container type for all input parameters for the `validateUserOp` function with signature `validateUserOp(address,(address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes),bytes32,uint256)` and selector `0xa19d19d5`
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
    #[ethcall(
        name = "validateUserOp",
        abi = "validateUserOp(address,(address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes),bytes32,uint256)"
    )]
    pub struct ValidateUserOpCall {
        pub account: ::ethers::core::types::Address,
        pub user_op: UserOperation,
        pub user_op_hash: [u8; 32],
        pub missing_account_funds: ::ethers::core::types::U256,
    }
    ///Container type for all input parameters for the `withdrawTo` function with signature `withdrawTo(address,uint256)` and selector `0x205c2878`
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
    #[ethcall(name = "withdrawTo", abi = "withdrawTo(address,uint256)")]
    pub struct WithdrawToCall {
        pub to: ::ethers::core::types::Address,
        pub amount: ::ethers::core::types::U256,
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
    pub enum MockEntryPointCalls {
        BalanceOf(BalanceOfCall),
        DepositTo(DepositToCall),
        ValidateUserOp(ValidateUserOpCall),
        WithdrawTo(WithdrawToCall),
    }
    impl ::ethers::core::abi::AbiDecode for MockEntryPointCalls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <BalanceOfCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::BalanceOf(decoded));
            }
            if let Ok(decoded) = <DepositToCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::DepositTo(decoded));
            }
            if let Ok(decoded) =
                <ValidateUserOpCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::ValidateUserOp(decoded));
            }
            if let Ok(decoded) = <WithdrawToCall as ::ethers::core::abi::AbiDecode>::decode(data) {
                return Ok(Self::WithdrawTo(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for MockEntryPointCalls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::BalanceOf(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::DepositTo(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::ValidateUserOp(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::WithdrawTo(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for MockEntryPointCalls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::BalanceOf(element) => ::core::fmt::Display::fmt(element, f),
                Self::DepositTo(element) => ::core::fmt::Display::fmt(element, f),
                Self::ValidateUserOp(element) => ::core::fmt::Display::fmt(element, f),
                Self::WithdrawTo(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<BalanceOfCall> for MockEntryPointCalls {
        fn from(value: BalanceOfCall) -> Self {
            Self::BalanceOf(value)
        }
    }
    impl ::core::convert::From<DepositToCall> for MockEntryPointCalls {
        fn from(value: DepositToCall) -> Self {
            Self::DepositTo(value)
        }
    }
    impl ::core::convert::From<ValidateUserOpCall> for MockEntryPointCalls {
        fn from(value: ValidateUserOpCall) -> Self {
            Self::ValidateUserOp(value)
        }
    }
    impl ::core::convert::From<WithdrawToCall> for MockEntryPointCalls {
        fn from(value: WithdrawToCall) -> Self {
            Self::WithdrawTo(value)
        }
    }
    ///Container type for all return fields from the `balanceOf` function with signature `balanceOf(address)` and selector `0x70a08231`
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
    pub struct BalanceOfReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `validateUserOp` function with signature `validateUserOp(address,(address,uint256,bytes,bytes,uint256,uint256,uint256,uint256,uint256,bytes,bytes),bytes32,uint256)` and selector `0xa19d19d5`
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
    pub struct ValidateUserOpReturn {
        pub validation_data: ::ethers::core::types::U256,
    }
}
