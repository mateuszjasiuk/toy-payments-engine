# Toy Payments Engine

A simple transaction processing engine that handles deposits, withdrawals, disputes, and chargebacks for clients.

## Usage

Build:

```bash
cargo build --release
```

Run:

```bash
cargo run -- transactions.csv > accounts.csv
```

Test:

```bash
cargo test
```

**Unit tests** - For specific edge cases (disputes, chargebacks, locked accounts, negative balances)

**Property tests** - To verify invariants (`available + held = total`, `held >= 0`) across random transactions using `proptest`

**E2E test** - Full CSV processing scenario with known input/output

## Input Format

CSV with columns: `type`, `client`, `tx`, `amount`

Supported transaction types:

- `deposit` - Credit to account
- `withdrawal` - Debit from account
- `dispute` - Challenge a transaction
- `resolve` - Resolve a dispute
- `chargeback` - Reverse a transaction and lock account

## Output Format

CSV with columns: `client`, `available`, `held`, `total`, `locked`

## Design Decisions

### **Decision:** Only deposit transactions can be disputed. Withdrawal transactions cannot be disputed.

**Reasoning:**

- The spec says that "funds should be held" during a dispute, which tells us that the funds must be present in the client balance
- Matches the fraud scenario described (deposit fraudulent funds, withdraw crypto, reverse the deposit)

**Impact:**

- Only deposits are stored in the `deposits` HashMap(reduces the memory usage)

### **Decision:** Each transaction can only be disputed once: Normal -> UnderDispute -> (Resolved OR ChargedBack).

**Reasoning:**

- Prevents dispute spam
- Simplifies state transitions
- Disputes are final

### **Decision:** The `available` field can go negative when disputing a deposit after funds have been withdrawn.

**Reasoning:**

- Replicates the fraud scenario: deposit $100, withdraw $100, dispute the deposit
- After dispute: available = -$100, held = $100, total = $0
- The spec does not explicitly disallows this

### **Decision:** Client IDs and Transaction IDs are stored both as HashMap keys and within their structs.

**Reasoning:**

- Consistency for all entity types
- Small memory impact (2-6 bytes per entry)

### **Decision:** Only deposits and withdrawals are blocked when client is locked

**Reasoning:**

- Dispute/resolve/chargeback operations on existing transactions must still be allowed
- A client might have multiple deposits under dispute - chargebacking one locks the account, but other disputes still need to be resolved

### **Decision:** Invalid transactions (non-existent tx_id, mismatched client_id, wrong status, insufficient funds, etc.) are silently ignored.

**Reasoning:**

- Spec says "you can ignore it and assume this is an error on our partners side"
- Keeps the output clean
- Does not block the processing

### **Decision:** Use `rust_decimal::Decimal`.

**Reasoning:**

- Avoids floating-point precision errors
- Spec requires precision of up to 4 decimal places

### **Decision:** Process CSV file line-by-line.

**Reasoning:**

- Spec mentions efficiency: "Can you stream values through memory as opposed to loading the entire data set upfront?"
- We try to store only the needed data in the memory (clients, deposits)

### **Decision:** Trust the spec's guarantee that transaction IDs are globally unique.

**Reasoning:**

- Spec says that transaction IDs are "globally unique"
- Otherwise we would have to store all the tx ids in a HashSet(this would increase memory footprint)

### **Decision:** Assume transaction amounts won't cause decimal overflow.

**Reasoning:**

- `rust_decimal` supports values up to ~10^28
- Realistic transaction amounts should be lower

### Decision: Use in-memory HashMaps instead of SQLite or embedded database.

**Reasoning:**

- Simpler implementation for a 2-3 hour exercise
- HashMaps provide O(1) lookups for clients and deposits
- Memory footprint scales only with unique clients + deposits
- For production, SQLite would be better for:
  - Persistence across restarts
  - Stored data larger than available memory
