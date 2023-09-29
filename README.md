# Price feed Oracle with Ink! Smart Contracts and Phat contracts

The Phat Contract `PriceFeed` fetches the prices from Coingecko api and sends them into Ink! Smart Contract

## Phat Contract `PriceFeed` 

To deploy this Phat Contract you can build the contract or use existing artifacts

More information here: [/phat/contracts/price_feed/README.md](./phat/contracts/price_feed)

### Build the contract 

To build the contract:
```bash
cd phat/contracts/price_feed
cargo contract build
```

### Use existing artifacts
All artifacts are here: [phat/artifacts](phat/artifacts)


## Ink! Smart Contract `PriceFeedConsumer`

To deploy this Ink! Smart Contract you can build the contract or use existing artifacts

### Build the contract

To build the contract:
```bash
cd ./ink/contracts/price_feed_consumer
cargo contract build
```

More information here: [/ink/contracts/price_feed_consumer/README.md](./ink/contracts/price_feed_consumer)

### Use existing artifacts
All artifacts are here: [ink/artifacts](ink/artifacts)

