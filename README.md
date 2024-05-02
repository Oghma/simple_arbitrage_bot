# Simple arbitrage bot

A very simple arbitrage bot between dydx and aevo

## Usage
To change the bot configuration, open `.env` and change the settings
- `AEVO_SYMBOL`: pair symbol for Aevo
- `AEVO_FEE`: Aevo trading fee. If set to 0 no fees are applied. 0.015 means 0.015%
- `DYDX_SYMBOL`: pair symbol for DyDx
- `DYDX_FEE`: DyDx trading fee. If set to 0 no fees are applied. 0.05 means 0.05%
- `STARTING_VALUE`: Starting wallets budget. Expressed in quote token (USD in
  our case). At the start is divided 50/50 between base and quote tokens
- `PERSISTENT_TRADES`: If true, virtual trades are applied to the order book.

After changing the configuration launch the bot with `cargo run`
