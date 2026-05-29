# ALGORITHMS_OF_UNIVERSE.md v2.0

## Status
Draft for waters-node v0.6.0.

## Working Plan
1. Startup reads bortal journal first.
2. Node identity derives from node_name + pincode + entropy.
3. Slime mold DTN weights govern routing and decay.
4. LeWM enters binary as CPU-friendly latent predictor.
5. ID and Free Energy replace D_f in all proofs and metrics.
6. Tamagotchi controls wake/sleep/exit lifecycle.
7. SPIFFE/SPIRE provides node identity and mTLS.

## Startup Order
1. Read `bortal` journal.
2. Load kvstore/redis state.
3. Restore session and node state.
4. Initialize identity.
5. Initialize DTN and peer registry.
6. Initialize LeWM and cognitive layers.

## Notes
- Keep all secrets on this server.
- Commit major changes incrementally.
- Push only when necessary.
