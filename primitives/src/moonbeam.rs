use std::os::unix::prelude::OsStringExt;
use web3::contract::{Error as Web3ContractErr, Contract, tokens::{Tokenize, Detokenize} };
pub use super::*;
use web3::{self as web3, api::Eth, transports::{WebSocket, Http}, ethabi, Transport };


pub const MOONBEAM_SCAN_SPAN: usize = 10;
// TODO: move it to config file
pub const MOONBEAM_LISTENED_EVENT: &'static str = "AddProof";
pub const MOONBEAM_BLOCK_DURATION: u64 = 12;
pub const MOONBEAM_TRANSACTION_CONFIRMATIONS: usize = 2;


// TODO: transform
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MoonbeamConfig {
    pub url: String,
    // where users add their proofs and emit `AddProof` event
    pub read_contract: String,
    // where keeper submit the verify result
    pub write_contract: String,
    pub private_key: String,
}

pub struct MoonbeamClient<T: Transport> {
    inner: Web3<T>
}

impl<T: Transport> MoonbeamClient<T> {

    pub fn web3(self) -> Web3<T> {
        self.inner
    }

    // get proof contract
    pub fn proof_contract(self, contract_addr: &str) -> Result<Contract<T>> {
        let address = utils::trim_address_str(contract_addr)?;
        let contract = Contract::from_json(
            self.web3().eth(),
            address,
            include_bytes!("../contracts/KiltProofs.json"),
        )?;
        Ok(contract)
    }

    // get submit verification contract
    pub fn aggregator_contract(self, contract_addr: &str) -> Result<Contract<T>> {
        let address = utils::trim_address_str(contract_addr)?;
        let contract = Contract::from_json(
            self.web3().eth(),
            address,
            include_bytes!("../contracts/SimpleAggregator.json"),
        )?;
        Ok(contract)
    }
}


pub mod utils {
    use super::*;
    pub async fn events<T: Transport, R: Detokenize>(
        web3: Eth<T>,
        contract: &Contract<T>,
        event: &str,
        from: Option<U64>,
        to: Option<U64>,
    ) -> std::result::Result<Vec<(R, Log)>, Web3ContractErr> {
        fn to_topic<A: Tokenize>(x: A) -> ethabi::Topic<ethabi::Token> {
            let tokens = x.into_tokens();
            if tokens.is_empty() {
                ethabi::Topic::Any
            } else {
                tokens.into()
            }
        }

        let res = contract.abi().event(event).and_then(|ev| {
            let filter = ev.filter(ethabi::RawTopicFilter {
                topic0: to_topic(()),
                topic1: to_topic(()),
                topic2: to_topic(()),
            })?;
            Ok((ev.clone(), filter))
        });
        let (ev, filter) = match res {
            Ok(x) => x,
            Err(e) => return Err(e.into()),
        };

        let mut builder = FilterBuilder::default().topic_filter(filter);
        if let Some(f) = from {
            builder = builder.from_block(BlockNumber::Number(f));
        }
        if let Some(t) = to {
            builder = builder.to_block(BlockNumber::Number(t));
        }

        let filter = builder.build();

        let logs = web3.logs(filter).await?;
        logs.into_iter()
            .map(move |l| {
                let log = ev.parse_log(ethabi::RawLog {
                    topics: l.topics.clone(),
                    data: l.data.0.clone(),
                })?;

                Ok((
                    R::from_tokens(log.params.into_iter().map(|x| x.value).collect::<Vec<_>>())?,
                    l,
                ))
            })
            .collect::<_>()
    }


    pub(super) fn trim_address_str(addr: &str) -> Result<Address> {
        let addr = if addr.starts_with("0x") {
            &addr[2..]
        } else {
            addr
        };
        let hex_res =
            hex::decode(addr).map_err(|e| Error::InvalidEthereumAddress(format!("{:}", e)))?;
        // check length
        if hex_res.len() != 20 {
            return Err(Error::InvalidEthereumAddress(format!(
                "Address is not equal to 20 bytes: {:}",
                addr
            )))
        }
        let address = Address::from_slice(&hex_res);
        Ok(address)

    }
}


#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Web3 Client Error, err: {0}")]
    Web3Error(#[from] web3::Error),

    #[error("Web3 Contract Error, err: {0}")]
    Web3ContractError(#[from] web3::contract::Error),

    #[error("Ethereum Abi Error, err: {0}")]
    EthAbiError(#[from] web3::ethabi::Error),

    #[error("Invalid Ethereum Address: {0}")]
    InvalidEthereumAddress(String),
}

pub type Result<T> = std::result::Result<T, Error>;


#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MoonbeamClient, U64};
    use web3::transports::Http;

    #[test]
    fn test_cargo_env_variables() {
        let contract_name = "KiltProofs";
        let bytes = include_bytes!("../contracts/KiltProofs.json");
        assert!(bytes.len() != 0);
    }

    #[tokio::test]
    async fn event_parse_should_work() {
        let web3 = Web3::new(Http::new("http://localhost:8545").unwrap());
        let bytecode = include_str!("../contracts/SimpleEvent.bin");
        let accounts = web3.eth().accounts().await.unwrap();
        let contract = Contract::deploy(web3.eth(), include_bytes!("../contracts/SimpleEvent.abi"))
            .unwrap()
            .confirmations(1)
            .execute(bytecode, (), accounts[0])
            .await
            .unwrap();

    }
}