Feature: Authentication Enforcement
  Verify that authentication is properly enforced across all protected endpoints

  @serial
  Scenario: BFF endpoints require valid Bearer token
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    When I send a POST request to /bff/internal/kyc/sessions without authentication
    Then the response status is 401

  @serial
  Scenario: BFF endpoints reject non-Bearer authentication
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    When I send a POST request to /bff/internal/kyc/sessions with Basic auth
    Then the response status is 401

  @serial
  Scenario: BFF endpoints reject invalid Bearer token
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    When I send a POST request to /bff/internal/kyc/sessions with an invalid Bearer token
    Then the response status is 401

  @serial
  Scenario: Staff endpoints require authentication
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    When I send a GET request to /staff/api/kyc/instances without authentication
    Then the response status is 401

  @serial
  Scenario: Valid authentication allows access
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up
    When I send a POST request to /bff/internal/kyc/sessions with valid authentication
    Then the response status is not 401

  @serial
  Scenario: Health endpoint bypasses authentication
    Given the e2e test environment is initialized
    When I send a GET request to /health without authentication
    Then the response status is 200

  @serial
  Scenario: Health endpoint allows invalid Bearer token
    Given the e2e test environment is initialized
    When I send a GET request to /health with an invalid Bearer token
    Then the response status is 200

  @serial
  Scenario: Non-existent endpoint returns 404 even with invalid Bearer
    Given the e2e test environment is initialized
    When I send a GET request to /does-not-exist-e2e with an invalid Bearer token
    Then the response status is 404