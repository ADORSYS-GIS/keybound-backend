# State Machines & Flows 🔄

## Why?
Complex flows like KYC and Phone OTP verification are prone to errors and race conditions! By using a robust state machine, we can manage these transitions safely and consistently every single time! ✨

## Actual
Our state machines are powered by a generic engine that tracks instances, events, and step attempts in our PostgreSQL database. 🛠️

### KYC Phone OTP Flow
This state machine handles phone number verification through SMS OTPs.

```mermaid
stateDiagram-v2
    [*] --> ACTIVE: User Starts Session
    ACTIVE --> RUNNING: User Issues OTP
    RUNNING --> WAITING_INPUT: OTP Sent Successfully
    WAITING_INPUT --> RUNNING: User Verifies OTP
    RUNNING --> COMPLETED: OTP Verified
    WAITING_INPUT --> RUNNING: User Retries OTP (if tries left)
    WAITING_INPUT --> FAILED: Tries Exhausted
```

### KYC First Deposit Flow
This more complex flow involves staff interaction and external service registration.

```mermaid
stateDiagram-v2
    [*] --> ACTIVE: User Requests Deposit
    ACTIVE --> WAITING_INPUT: Awaiting Payment Confirmation (Staff)
    WAITING_INPUT --> RUNNING: Staff Confirms Payment
    RUNNING --> WAITING_INPUT: Awaiting Approval (Staff)
    WAITING_INPUT --> RUNNING: Staff Approves Deposit
    RUNNING --> REGISTERING_CUSTOMER: Automated Step (Worker)
    REGISTERING_CUSTOMER --> APPROVING_AND_DEPOSITING: Automated Step (Worker)
    APPROVING_AND_DEPOSITING --> COMPLETED: Success! 🎉
    REGISTERING_CUSTOMER --> FAILED: Registration Error
```

## Constraints
- Every step is tracked in `sm_step_attempt`.
- Transitions are triggered by events recorded in `sm_event`.
- Context data (like deposit amounts or OTP hashes) is stored as JSON in the instance.

## Findings
The generic state-machine engine is incredibly powerful! We can easily retry failed automated steps from the Staff API without manually modifying database rows! 🛠️ It's saved us so much time! 🥳

## How to?
To add a new state machine:
1. Define the kind and steps in `app/crates/backend-server/src/state_machine/types.rs`.
2. Implement the engine logic in `app/crates/backend-server/src/state_machine/engine.rs`.
3. Add the API surface calls in BFF or Staff controllers. 🛠️

## Conclusion
State machines are our best friend when it comes to managing complex business logic! They keep our code clean, reliable, and observable! 🥳✨
