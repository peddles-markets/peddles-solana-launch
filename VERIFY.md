# Verifying the deployed program

The program at `4jTFPcqrwzxP9Ve7xo6dUk4oYWbfbi78qm63tX1JC4tL` is built from this tree with `solanafoundation/solana-verifiable-build`
for Solana 4.1.2 (pinned by digest in `scripts/verifiable-build.sh`), SBPF **v3**.

```
solana-verify verify-from-repo -u https://api.mainnet-beta.solana.com \
  --program-id 4jTFPcqrwzxP9Ve7xo6dUk4oYWbfbi78qm63tX1JC4tL --library-name peddles_launch --arch v3 \
  --commit-hash <commit> https://github.com/peddles-markets/peddles-solana-launch
```

`--arch v3` matters: solana-verify's default is v0, which builds a different binary.
This tree is exported from Peddles' private monorepo, where it is maintained.
