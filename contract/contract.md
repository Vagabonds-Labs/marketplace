# Declaring OpenZeppeling ERC1155 contract to Zepolia

## Requisites

1. Latest `starkli` version (You can update it using `starkliup`).
2. `scarb` version `2.5.4`. To install it use:
   `curl --proto '=https' --tlsv1.2 -sSf https://docs.swmansion.com/scarb/install.sh | sh -s -- -v 2.5.4`

## Compile contract

Use these commands to compile the OpenZeppelin ERC1155 contract:

```
git clone git@github.com:OpenZeppelin/cairo-contracts.git
git checkout 292417dfa84e56b4accb6e2540a1aca0e6cd6219
cd cairo-contracts
scarb build
```

Make sure that `scarb` generated the Sierra contract artifact in `target/dev/openzeppelin_ERC1155.contract_class.json`.

Then verify that the class-hash matches the one we expect with `starkli class-hash target/dev/openzeppelin_ERC1155.contract_class.json`.
The output must be: `0x0120d1f2225704b003e77077b8507907d2a84239bef5e0abb67462495edd644f`.

## Create keystore

Use these steps to create a starknet keyguard:
```
starkli signer keystore new <keystore_json_file>
export STARKNET_KEYSTORE=$PWD/<keystore_json_file>
```

## Deploy account

Use these steps to deploy an account to Sepolia:
```
starkli account oz init <account_json_file>
starkli account deploy <account_json_file> --network sepolia
```

## Declare contract

Use these steps to declare the OpenZeppelin ERC1155 contract that was compiled before.
```
starkli declare --watch --compiler-version 2.5.4 --account <account_json_file> target/dev/openzeppelin_ERC1155.contract_class.json --network sepolia
```