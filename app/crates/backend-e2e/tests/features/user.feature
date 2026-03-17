Feature: User Endpoints
  Verify BFF user endpoint behavior

  Background:
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    And the SMS sink is reset

  @serial
  Scenario: Get user returns correct user ID
    When I get the current user
    Then the response status is 200
    And the response contains the correct user ID

  @serial
  Scenario: Initial KYC level is NONE
    When I get the KYC level
    Then the response status is 200
    And the KYC level is "NONE"
    And phoneOtpVerified is false
    And firstDepositVerified is false

  @serial
  Scenario: KYC level updates after phone OTP verification
    Given I complete phone OTP verification
    When I get the KYC level
    Then the response status is 200
    And the KYC level is "PHONE_OTP_VERIFIED"
    And phoneOtpVerified is true

  @serial
  Scenario: KYC summary reflects phone OTP completion
    Given I complete phone OTP verification
    When I get the KYC summary
    Then the response status is 200
    And the KYC level is "PHONE_OTP_VERIFIED"
    And phoneOtpStatus is "COMPLETED"

  @serial
  Scenario: KYC level updates after first deposit
    Given I complete phone OTP verification
    And I complete first deposit verification
    When I get the KYC level
    Then the response status is 200
    And the KYC level contains "PHONE_OTP_VERIFIED"
    And the KYC level contains "FIRST_DEPOSIT_VERIFIED"
    And firstDepositVerified is true