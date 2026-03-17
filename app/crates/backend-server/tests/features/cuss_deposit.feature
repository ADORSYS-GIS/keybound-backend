Feature: CUSS Deposit Flow
  Complete flow for user registration and deposit through CUSS integration

  @serial
  Scenario: Successful CUSS deposit flow with metadata persistence
    Given a user with id "usr_test_001" exists
    And a CUSS mock server is running
    And the CUSS register endpoint returns success with fineractClientId 12345 and savingsAccountId 67890
    And the CUSS approve endpoint returns success with transactionId 99999
    When the CHECK_USER_EXISTS step executes with session context:
      | phone_number  | +237690000000 |
      | full_name     | Test User     |
      | deposit_amount| 5000.0        |
      | userId        | usr_test_001  |
    Then the CHECK_USER_EXISTS step completes with Done outcome
    When the VALIDATE_DEPOSIT step executes with flow context containing user_exists true
    Then the VALIDATE_DEPOSIT step completes with Done outcome and valid true
    When the CUSS_REGISTER_CUSTOMER step executes with session context:
      | phone_number  | +237690000000 |
      | full_name     | Test User     |
    Then the CUSS_REGISTER_CUSTOMER step completes with Done outcome
    And the fineractClientId in output is 12345
    When the CUSS_APPROVE_AND_DEPOSIT step executes with savingsAccountId 67890 and deposit_amount 5000.0
    Then the CUSS_APPROVE_AND_DEPOSIT step completes with Done outcome
    And the transactionId in output is 99999
    And the savingsAccountId in output is 67890
    And the user metadata contains fineractClientId 12345
    And the user metadata contains savingsAccountId 67890
    And the user metadata contains deposit_transaction_id 99999
    And the user metadata contains cuss_registration_status "COMPLETED"
    And the user metadata contains cuss_approval_status "COMPLETED"

  @serial
  Scenario: CUSS register retryable on 5xx errors
    Given a CUSS mock server is running
    And the CUSS register endpoint returns error with status 503 and message "Service unavailable"
    When the CUSS_REGISTER_CUSTOMER step executes with session context:
      | phone_number  | +237690000000 |
      | full_name     | Test User     |
    Then the CUSS_REGISTER_CUSTOMER step completes with Failed outcome
    And the error message contains "503"
    And the failure is retryable

  @serial
  Scenario: CUSS register non-retryable on 4xx errors
    Given a CUSS mock server is running
    And the CUSS register endpoint returns error with status 400 and message "Invalid request"
    When the CUSS_REGISTER_CUSTOMER step executes with session context:
      | phone_number  | +237690000000 |
      | full_name     | Test User     |
    Then the CUSS_REGISTER_CUSTOMER step completes with Failed outcome
    And the error message contains "400"
    And the failure is not retryable