Feature: SMS Error Handling
  Verify SMS transient and permanent error behavior

  Background:
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    And the SMS sink is reset

  @serial
  Scenario: Transient SMS error retries until success
    Given I inject a transient SMS fault with status 503
    And I create a PHONE_OTP session
    And I create a phone OTP step
    When I issue an OTP to phone number +237690000099
    Then the response status is 200
    And I receive an OTP within 45 seconds

  @serial
  Scenario: Permanent SMS error does not deliver OTP
    Given I inject a permanent SMS fault with status 400
    And I create a PHONE_OTP session
    And I create a phone OTP step
    When I issue an OTP to phone number +237690000100
    Then the response status is 200
    And I do not receive an OTP within 12 seconds