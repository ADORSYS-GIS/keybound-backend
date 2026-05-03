Feature: ID Document Flow
  Validate the v2 `id_document` flow end-to-end through the BFF surface

  Background:
    Given the e2e test environment is initialized
    And I have a valid authentication token
    And the database fixtures are set up

  @serial
  Scenario: Flow Initiation - ID Document KYC Flow
    Given a registered device with JWT token
    When I send a POST request to /bff/sessions with valid authentication
    And the response status is 200
    And the response contains session ID
    When I send a POST request to /bff/sessions/{session_id}/flows with body:
      """
      {
        "flowType": "id_document"
      }
      """
    Then the response status is 200
    And the response contains flow ID
    And sm_instance row persisted for KYC_ID_DOCUMENT with import_id = 1

  @serial
  Scenario: Document Upload - Valid ID Document Upload
    Given a registered device with JWT token
    And I have created an id_document session
    When I upload valid id_document documents for session "{session_id}"
      """
      {
        "front": { "filename": "id_front.png", "base64": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==", "mime": "image/png" },
        "back":  { "filename": "id_back.png", "base64": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==", "mime": "image/png" }
      }
      """
    Then the response status is 200
    And sm_event entries exist for DocumentUploaded event
    And session still exists in sm_instance

  @serial
  Scenario: Manual Review - Staff Approval of ID Document
    Given an id_document session AWAITING_REVIEW
    When I approve the id_document session via staff API
    Then the response status is 200
    And sm_event entries exist for DocumentApproved event
    And session state transitions to DocumentApproved

  @serial
  Scenario: Manual Review - Staff Rejection of ID Document
    Given an id_document session AWAITING_REVIEW
    When I reject the id_document session via staff API with reason "Document quality insufficient"
    Then the response status is 200
    And sm_event entries exist for DocumentRejected event
    And session state transitions to DocumentRejected

  @serial
  Scenario: Flow Completion - User Profile Update After Approval
    Given an approved id_document session
    When I get the current user
    Then the response status is 200
    And the user profile contains id_document verification status
    When I get completed KYC
    Then the response status is 200
    And completed KYC contains flow "id_document"
    And KYC tier level is updated appropriately