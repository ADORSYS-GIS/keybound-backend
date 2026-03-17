Feature: BFF Deposit and OTP Flow
  Complete flow for deposit requests and OTP verification

  Background:
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    And the SMS sink is reset

  @serial
  Scenario: Create and verify phone OTP
    When I create a PHONE_OTP session
    Then the response status is 201
    When I create a phone OTP step
    Then the response status is 201
    And the step status is NOT_STARTED

  @serial
  Scenario: Issue OTP and verify step transitions
    Given I create a PHONE_OTP session
    And I create a phone OTP step
    When I issue an OTP to phone number +237690000033
    Then the response status is 200
    And I receive an OTP within 30 seconds

  @serial
  Scenario: Wrong OTP verification fails
    Given I create a PHONE_OTP session
    And I create a phone OTP step
    And I issue an OTP to phone number +237690000033
    When I verify OTP with code "000000"
    Then the response status is 200
    And the verification result is "INVALID"
    And the step status is IN_PROGRESS

  @serial
  Scenario: Correct OTP verification succeeds
    Given I create a PHONE_OTP session
    And I create a phone OTP step
    And I issue an OTP to phone number +237690000033
    And I receive an OTP within 30 seconds
    When I verify OTP with the received code
    Then the response status is 200
    And the verification result is "VERIFIED"
    And the step status is VERIFIED