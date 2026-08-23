# V0 forecast policy

## Definition

For a single-currency scenario, RunwayClock simulates:

```text
closing balance for each future day
  = prior closing balance
  + explicit income occurring that day
  - explicit expense occurring that day
  - historical baseline spending allocated to that day
```

The zero-date is the first day for which:

```text
closing balance <= reserve
```

The imported latest balance is treated as an end-of-day balance. Simulation
starts on the following day.

## Actual evidence included in the baseline

An unannotated negative transaction is included. The following user-confirmed
interpretations change that behavior:

| Interpretation | Negative transaction | Positive transaction |
|---|---:|---:|
| fixed / variable / irregular recurrent | included | excluded |
| unknown / unreviewed | included | excluded |
| transfer | excluded | excluded |
| exceptional | excluded | excluded |
| income | excluded | excluded |
| refund | excluded | subtracts from observed expense |

The observation window is the first through last actual transaction date,
inclusive. Empty days are intentionally part of the denominator. This estimates
spending per elapsed day, not per day on which a transaction happened.

Irregular transactions remain included unless the user says they are truly
exceptional. This avoids the systematically optimistic result produced by
discarding every apparent one-off.

## Forecast evidence

Forecast rules live in their own table and never appear in the actual ledger.
V0 supports:

- one-time income or expense on an exact date;
- monthly income or expense on a chosen day;
- an optional inclusive end date;
- source, confidence, and evidence labels.

The CLI only creates `user_confirmed` rules at confidence 1.0. There is no salary
inference. A historical salary deposit affects the actual balance but does not
become future no-work income.

## Arithmetic

All amounts use signed 64-bit minor units, such as `650000 TRY` for ₺6,500.00.
Historical daily spending remains the rational value:

```text
included expense minor units / observed elapsed days
```

The simulation carries the division remainder from day to day. It therefore
neither uses floating point nor loses fractional minor units through daily
rounding.

Monthly flows clamp days 29–31 to the last valid day of shorter months. Rules
occurring on the same day are netted into that day's closing balance.

## Confidence and warnings

The widget's initial confidence label measures only history coverage:

- low: fewer than 30 observed days;
- medium: 30–179 observed days;
- high: at least 180 observed days.

It is not a statistical confidence interval. The full result warns when less
than 30 days are available and when the reserve is not reached within the
100-year default horizon.

## Known model risks

The baseline can be too pessimistic when history contains internal transfers,
credit-card payments that duplicate card purchases, or genuinely exceptional
costs. It can be too optimistic when the observed period missed annual or rare
costs, when current prices have risen, or when known future expenses were not
entered.

The next inference layer should decompose spending into fixed recurrent,
variable recurrent, irregular recurrent as a class, and exceptional components.
Those outputs must remain proposals with evidence and estimated runway effect.
