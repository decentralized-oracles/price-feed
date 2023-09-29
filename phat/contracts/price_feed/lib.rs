#![cfg_attr(not(feature = "std"), no_std, no_main)]

extern crate alloc;
extern crate core;

pub use self::price_feed::PriceFeedRef;

#[ink::contract(env = pink_extension::PinkEnvironment)]
mod price_feed {
    use alloc::{collections::BTreeMap, format, string::String, string::ToString, vec, vec::Vec};
    use ink::env::debug_println;

    use pink_extension::chain_extension::signing;
    use pink_extension::{error, ResultExt};
    use scale::{Decode, Encode};

    use fixed::types::U80F48 as Fp;

    use phat_offchain_rollup::clients::ink::{Action, ContractId, InkRollupClient};

    pub type TradingPairId = u32;

    /// Message to request the price of the trading pair
    /// message pushed in the queue by this contract and read by the offchain rollup
    #[derive(Encode, Decode)]
    struct PriceRequestMessage {
        /// id of the pair (use as key in the Mapping)
        trading_pair_id: TradingPairId,
        /// trading pair like 'polkatot/usd'
        /// Note: it will be better to not save this data in the storage
        token0: String,
        token1: String,
    }
    /// Message sent to provide the price of the trading pair
    /// response pushed in the queue by the offchain rollup and read by this contract
    #[derive(Encode, Decode)]
    struct PriceResponseMessage {
        /// Type of response
        resp_type: u8,
        /// id of the pair
        trading_pair_id: TradingPairId,
        /// price of the trading pair
        price: Option<u128>,
        /// when the price is read
        err_no: Option<u128>,
    }

    /// Type of response when the offchain rollup communicate with this contract
    //const TYPE_ERROR: u8 = 0;
    //const TYPE_RESPONSE: u8 = 10;
    const TYPE_FEED: u8 = 11;

    #[ink(storage)]
    pub struct PriceFeed {
        owner: AccountId,
        config: Option<Config>,
        /// Key for signing the rollup tx.
        attest_key: [u8; 32],
    }

    #[derive(Encode, Decode, Debug)]
    #[cfg_attr(
        feature = "std",
        derive(scale_info::TypeInfo, ink::storage::traits::StorageLayout)
    )]
    struct Config {
        /// The RPC endpoint of the target blockchain
        rpc: String,
        pallet_id: u8,
        call_id: u8,
        /// The rollup anchor address on the target blockchain
        contract_id: ContractId,
        /// Key for sending out the rollup meta-tx. None to fallback to the wallet based auth.
        sender_key: Option<[u8; 32]>,
    }

    #[derive(Encode, Decode, Debug)]
    #[repr(u8)]
    #[cfg_attr(feature = "std", derive(scale_info::TypeInfo))]
    pub enum Error {
        BadOrigin,
        NotConfigured,
        InvalidKeyLength,
        InvalidAddressLength,
        NoRequestInQueue,
        FailedToCreateClient,
        FailedToCommitTx,
        FailedToFetchPrice,

        FailedToGetStorage,
        FailedToCreateTransaction,
        FailedToSendTransaction,
        FailedToGetBlockHash,
        FailedToDecode,
        InvalidRequest,
        FailedToCallRollup,
    }

    type Result<T> = core::result::Result<T, Error>;

    impl From<phat_offchain_rollup::Error> for Error {
        fn from(error: phat_offchain_rollup::Error) -> Self {
            error!("error in the rollup: {:?}", error);
            debug_println!("error in the rollup: {:?}", error);
            Error::FailedToCallRollup
        }
    }

    impl PriceFeed {
        #[ink(constructor)]
        pub fn default() -> Self {
            const NONCE: &[u8] = b"attest_key";
            let private_key = signing::derive_sr25519_key(NONCE);
            Self {
                owner: Self::env().caller(),
                attest_key: private_key[..32].try_into().expect("Invalid Key Length"),
                config: None,
            }
        }

        /// Gets the owner of the contract
        #[ink(message)]
        pub fn owner(&self) -> AccountId {
            self.owner
        }

        /// Gets the attestor address used by this rollup
        #[ink(message)]
        pub fn get_attest_address(&self) -> Vec<u8> {
            signing::get_public_key(&self.attest_key, signing::SigType::Sr25519)
        }

        /// Gets the ecdsa address used by this rollup in the meta transaction
        #[ink(message)]
        pub fn get_attest_ecdsa_address(&self) -> Vec<u8> {
            use ink::env::hash;
            let input = signing::get_public_key(&self.attest_key, signing::SigType::Ecdsa);
            let mut output = <hash::Blake2x256 as hash::HashOutput>::Type::default();
            ink::env::hash_bytes::<hash::Blake2x256>(&input, &mut output);
            output.to_vec()
        }

        /// Set attestor key.
        ///
        /// For dev purpose.
        #[ink(message)]
        pub fn set_attest_key(&mut self, attest_key: Option<Vec<u8>>) -> Result<()> {
            self.attest_key = match attest_key {
                Some(key) => key.try_into().or(Err(Error::InvalidKeyLength))?,
                None => {
                    const NONCE: &[u8] = b"attest_key";
                    let private_key = signing::derive_sr25519_key(NONCE);
                    private_key[..32]
                        .try_into()
                        .or(Err(Error::InvalidKeyLength))?
                }
            };
            Ok(())
        }

        /// Gets the sender address used by this rollup
        #[ink(message)]
        pub fn get_sender_address(&self) -> Option<Vec<u8>> {
            if let Some(Some(sender_key)) = self.config.as_ref().map(|c| c.sender_key.as_ref()) {
                let sender_key = signing::get_public_key(sender_key, signing::SigType::Sr25519);
                Some(sender_key)
            } else {
                None
            }
        }

        /// Gets the config
        #[ink(message)]
        pub fn get_target_contract(&self) -> Option<(String, u8, u8, ContractId)> {
            self.config
                .as_ref()
                .map(|c| (c.rpc.clone(), c.pallet_id, c.call_id, c.contract_id))
        }

        /// Configures the rollup target (admin only)
        #[ink(message)]
        pub fn config(
            &mut self,
            rpc: String,
            pallet_id: u8,
            call_id: u8,
            contract_id: Vec<u8>,
            sender_key: Option<Vec<u8>>,
        ) -> Result<()> {
            self.ensure_owner()?;
            self.config = Some(Config {
                rpc,
                pallet_id,
                call_id,
                contract_id: contract_id
                    .try_into()
                    .or(Err(Error::InvalidAddressLength))?,
                sender_key: match sender_key {
                    Some(key) => Some(key.try_into().or(Err(Error::InvalidKeyLength))?),
                    None => None,
                },
            });
            Ok(())
        }

        /// Transfers the ownership of the contract (admin only)
        #[ink(message)]
        pub fn transfer_ownership(&mut self, new_owner: AccountId) -> Result<()> {
            self.ensure_owner()?;
            self.owner = new_owner;
            Ok(())
        }

        fn fetch_coingecko_prices(
            trading_pairs: &[PriceRequestMessage],
        ) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
            let mut tokens = String::new();
            let mut currencies = String::new();

            let mut add_comma = false;
            for trading_pair in trading_pairs.iter() {
                if add_comma {
                    tokens.push(',');
                    currencies.push(',');
                } else {
                    add_comma = true;
                }
                tokens.push_str(trading_pair.token0.as_str());
                currencies.push_str(trading_pair.token1.as_str());
            }

            // Fetch the prices from CoinGecko.
            //
            // Supported tokens are listed in the detailed documentation:
            // <https://www.coingecko.com/en/api/documentation>
            let url = format!(
                "https://api.coingecko.com/api/v3/simple/price?ids={tokens}&vs_currencies={currencies}"
            );
            let headers = vec![("accept".into(), "application/json".into())];
            let resp = pink_extension::http_get!(url, headers);
            if resp.status_code != 200 {
                return Err(Error::FailedToFetchPrice);
            }
            // The response looks like:
            //      {
            //         "astar": {"usd": 0.06009},
            //         "bitcoin": {"usd": 25846},
            //         "ethereum": {"usd": 1630.07},
            //         "kusama": {"usd": 19.05},
            //         "moonbeam": {"usd": 0.182328},
            //         "pha": {"usd": 0.094045},
            //         "polkadot": {"usd": 4.23}
            //     }
            //

            let parsed: BTreeMap<String, BTreeMap<String, String>> =
                pink_json::from_slice(&resp.body)
                    .log_err("failed to parse json")
                    .or(Err(Error::FailedToDecode))?;

            Ok(parsed)
        }

        /// Processes a price request by a rollup transaction
        #[ink(message)]
        pub fn feed_prices(&self) -> Result<Option<Vec<u8>>> {
            let config = self.ensure_configured()?;
            let mut client = connect(config)?;

            // get all trading pairs
            let trading_pairs = get_trading_pairs();

            // fetch the price for this trading pair
            let prices = Self::fetch_coingecko_prices(&trading_pairs)?;

            // iter on all trading pairs
            for request in trading_pairs.iter() {
                if let Some(price) = prices
                    .get(&request.token0)
                    .and_then(|t| t.get(&request.token1))
                {
                    let fp = Fp::from_str(price)
                        .log_err("failed to parse real number")
                        .or(Err(Error::FailedToDecode))?;
                    let f = fp * Fp::from_num(1_000_000_000_000_000_000u128);

                    // build the payload
                    let payload = PriceResponseMessage {
                        resp_type: TYPE_FEED,
                        trading_pair_id: request.trading_pair_id,
                        price: Some(f.to_num()),
                        err_no: None,
                    };
                    // Attach the action to the transaction
                    client.action(Action::Reply(payload.encode()));
                }
            }
            // submit the transaction
            maybe_submit_tx(client, &self.attest_key, config.sender_key.as_ref())
        }

        /// Returns BadOrigin error if the caller is not the owner
        fn ensure_owner(&self) -> Result<()> {
            if self.env().caller() == self.owner {
                Ok(())
            } else {
                Err(Error::BadOrigin)
            }
        }

        /// Returns the config reference or raise the error `NotConfigured`
        fn ensure_configured(&self) -> Result<&Config> {
            self.config.as_ref().ok_or(Error::NotConfigured)
        }
    }

    fn get_trading_pairs() -> Vec<PriceRequestMessage> {
        vec![
            PriceRequestMessage {
                trading_pair_id: 1,
                token0: "bitcoin".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 2,
                token0: "ethereum".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 3,
                token0: "binancecoin".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 13,
                token0: "polkadot".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 171,
                token0: "kusama".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 147,
                token0: "astar".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 720,
                token0: "shiden".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 190,
                token0: "moonbeam".to_string(),
                token1: "usd".to_string(),
            },
            PriceRequestMessage {
                trading_pair_id: 384,
                token0: "pha".to_string(),
                token1: "usd".to_string(),
            },
        ]
    }

    fn connect(config: &Config) -> Result<InkRollupClient> {
        let result = InkRollupClient::new(
            &config.rpc,
            config.pallet_id,
            config.call_id,
            &config.contract_id,
        )
        .log_err("failed to create rollup client");

        match result {
            Ok(client) => Ok(client),
            Err(e) => {
                error!("Error : {:?}", e);
                Err(Error::FailedToCreateClient)
            }
        }
    }

    fn maybe_submit_tx(
        client: InkRollupClient,
        attest_key: &[u8; 32],
        sender_key: Option<&[u8; 32]>,
    ) -> Result<Option<Vec<u8>>> {
        let maybe_submittable = client
            .commit()
            .log_err("failed to commit")
            .map_err(|_| Error::FailedToCommitTx)?;

        if let Some(submittable) = maybe_submittable {
            let tx_id = if let Some(sender_key) = sender_key {
                // Prefer to meta-tx
                submittable
                    .submit_meta_tx(attest_key, sender_key)
                    .log_err("failed to submit rollup meta-tx")?
            } else {
                // Fallback to account-based authentication
                submittable
                    .submit(attest_key)
                    .log_err("failed to submit rollup tx")?
            };
            return Ok(Some(tx_id));
        }
        Ok(None)
    }

    #[cfg(test)]
    mod tests {
        use ink::env::debug_println;

        use super::*;

        struct EnvVars {
            /// The RPC endpoint of the target blockchain
            rpc: String,
            pallet_id: u8,
            call_id: u8,
            /// The rollup anchor address on the target blockchain
            contract_id: ContractId,
            /// When we want to manually set the attestor key for signing the message (only dev purpose)
            attest_key: Vec<u8>,
            /// When we want to use meta tx
            sender_key: Option<Vec<u8>>,
        }

        fn get_env(key: &str) -> String {
            std::env::var(key).expect("env not found")
        }

        fn config() -> EnvVars {
            dotenvy::dotenv().ok();
            let rpc = get_env("RPC");
            let pallet_id: u8 = get_env("PALLET_ID").parse().expect("u8 expected");
            let call_id: u8 = get_env("CALL_ID").parse().expect("u8 expected");
            let contract_id: ContractId = hex::decode(get_env("CONTRACT_ID"))
                .expect("hex decode failed")
                .try_into()
                .expect("incorrect length");
            let attest_key = hex::decode(get_env("ATTEST_KEY")).expect("hex decode failed");
            let sender_key = std::env::var("SENDER_KEY")
                .map(|s| hex::decode(s).expect("hex decode failed"))
                .ok();

            EnvVars {
                rpc: rpc.to_string(),
                pallet_id,
                call_id,
                contract_id: contract_id.into(),
                attest_key,
                sender_key,
            }
        }

        #[ink::test]
        fn test_update_attestor_key() {
            let _ = env_logger::try_init();
            pink_extension_runtime::mock_ext::mock_all_ext();

            let mut price_feed = PriceFeed::default();

            // Secret key and address of Alice in localhost
            let sk_alice: [u8; 32] = [0x01; 32];
            let address_alice = hex_literal::hex!(
                "189dac29296d31814dc8c56cf3d36a0543372bba7538fa322a4aebfebc39e056"
            );

            let initial_attestor_address = price_feed.get_attest_address();
            assert_ne!(address_alice, initial_attestor_address.as_slice());

            price_feed.set_attest_key(Some(sk_alice.into())).unwrap();

            let attestor_address = price_feed.get_attest_address();
            assert_eq!(address_alice, attestor_address.as_slice());

            price_feed.set_attest_key(None).unwrap();

            let attestor_address = price_feed.get_attest_address();
            assert_eq!(initial_attestor_address, attestor_address);
        }

        fn init_contract() -> PriceFeed {
            let EnvVars {
                rpc,
                pallet_id,
                call_id,
                contract_id,
                attest_key,
                sender_key,
            } = config();

            let mut price_feed = PriceFeed::default();
            price_feed
                .config(rpc, pallet_id, call_id, contract_id.into(), sender_key)
                .unwrap();
            price_feed.set_attest_key(Some(attest_key)).unwrap();

            price_feed
        }

        #[ink::test]
        fn fetch_coingecko_prices() {
            let _ = env_logger::try_init();
            pink_extension_runtime::mock_ext::mock_all_ext();

            let trading_pairs = get_trading_pairs();

            let data = PriceFeed::fetch_coingecko_prices(&trading_pairs).unwrap();

            for (token, values) in data.iter() {
                for (currency, value) in values.iter() {
                    debug_println!("{}/{}: {}", token, currency, value);
                }
            }
        }

        #[ink::test]
        #[ignore = "the target contract must be deployed in local node or shibuya"]
        fn feed_prices() {
            let _ = env_logger::try_init();
            pink_extension_runtime::mock_ext::mock_all_ext();

            let price_feed = init_contract();

            let r = price_feed.feed_prices().expect("failed to feed prices");
            debug_println!("answer price: {r:?}");
        }
    }
}
