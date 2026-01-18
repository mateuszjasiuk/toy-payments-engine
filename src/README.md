## Assumptions

- Transaction amounts are reasonable and won't cause decimal overflow
  (Decimal supports up to ~10^28, far exceeding realistic transaction volumes)

## Design Decisions

### Memory Usage - ID Duplication

Client IDs and Transaction IDs are stored both as HashMap keys and within
their respective structs (Client, DepositTx, etc.). This creates a small
amount of duplication (~2-6 bytes per entry).

Alternative: IDs could be stored only as HashMap keys to eliminate
duplication. However, the current approach was chosen for:

- Code consistency across all entity types
- Improved readability (entities are self-contained)
- Negligible memory impact (a few MB even with millions of entries)

For production systems processing billions of transactions, this tradeoff
could be reconsidered.
