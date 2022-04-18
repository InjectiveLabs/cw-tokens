
# Cw20 ↔ Bank denom conversion
This Cw20 contract type extends Cw20 base contract with functionality to convert Cw20 tokens into Bank denom and vice versa.

## Advantages
- Cw20 tokens can be used as native tokens on the cosmos chain.
- Cw20 tokens can be transferred through IBC to other cosmos chains.
- Cw20 tokens can be transferred to Ethereum if the bridge from Cosmos to Ethereum exists. etc

## Implementation
To achieve this, the CW20-base contract should be extended with two new functions

### bank_to_cw20
`User can invoke this function to convert bank denom to cw20 tokens. Upon invocation,
the CW20 contract should verify the user bank balance.
transfer the bank denom to CW20 contract address from user balance.
Mint equal amount of CW20 tokens for user address.`

### cw20_to_bank
`User can invoke this function to convert cw20 tokens to bank denom. Upon invocation,
the CW20 contract should verify the user cw20 balance.
Burn cw20 tokens of the user
transfer equal amount of bank denom to user from CW20 contract address.`