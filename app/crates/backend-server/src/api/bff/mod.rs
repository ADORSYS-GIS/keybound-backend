#[async_trait]
impl Deposits<Error, JwtToken> for BackendApi {
    async fn internal_create_phone_deposit_request(
        &self,
        body: InternalCreatePhoneDepositRequestRequest,
        claims: JwtToken,
    ) -> Result<InternalCreatePhoneDepositRequestResponse, Error> {
        self.create_phone_deposit_request_flow(claims, body).await
    }

    async fn internal_get_phone_deposit_request(
        &self,
        path_params: InternalGetPhoneDepositRequestPathParams,
        claims: JwtToken,
    ) -> Result<InternalGetPhoneDepositRequestResponse, Error> {
        self.get_phone_deposit_request_flow(claims, path_params).await
    }
}
