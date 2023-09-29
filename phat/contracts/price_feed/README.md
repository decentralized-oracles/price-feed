# InkPriceFeed

Use Ink! Offchain Rollup to:
- Fetch the prices from Coingecko api
- Send the prices into ink! smart contracts 

## Build

To build the contract:

```bash
cargo contract build
```

## Run Integration tests

### Deploy the ink! smart contract `price_feed_consumer`

Before you can run the tests, you need to have an ink! smart contract deployed in a Substrate node with pallet-contracts.

#### Use the default ink! smart contract 

You can use the default smart contract deployed on Shibuya (`XzRQpoGkJhfULFJ1D7xtD1kHiraWffGTDdV3YfWSs3Rw28L`).

#### Or deploy your own ink! smart contract

You can build the smart contract 
```bash
cd ../../ink/contracts/price_feed_consumer
cargo contract build
```
And use Contracts-UI or Polkadot.js to deploy your contract and interact with it.
You will have to configure `alice` as attestor.

### Add trading pairs and push some requests

Use Contracts-UI or Polkadot.js to interact with your smart contract deployed on local node or Shibuya.

In Shibuya, there are already 3 trading pairs defined in the contracts `XzRQpoGkJhfULFJ1D7xtD1kHiraWffGTDdV3YfWSs3Rw28L`.
 - id 11 for the pair `polkadot`/`usd`
 - id 12 for `astar`/`usd`
 - id 13 for `pha`/`usd`

### Run the integration tests
And finally execute the following command to start integration tests execution.

```bash
cargo test  -- --ignored --test-threads=1
```

### Parallel in Integration Tests

The flag `--test-threads=1` is necessary because by default [Rust unit tests run in parallel](https://doc.rust-lang.org/book/ch11-02-running-tests.html).
There may have a few tests trying to send out transactions at the same time, resulting
conflicting nonce values.
The solution is to add `--test-threads=1`. So the unit test framework knows that you don't want
parallel execution.

### Enable Meta-Tx

Meta transaction allows the Phat Contract to submit rollup tx with attest key signature while using
arbitrary account to pay the gas fee. To enable meta tx in the unit test, change the `.env` file
and specify `SENDER_KEY`.
