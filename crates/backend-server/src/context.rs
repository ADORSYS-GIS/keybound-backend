use http::Request;
use swagger::auth::{Authorization, Scopes};
use swagger::{Has, XSpanIdString};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub struct KcContext {
    x_span_id: XSpanIdString,
}

impl KcContext {
    pub fn from_request<B>(req: &Request<B>) -> Self {
        Self {
            x_span_id: XSpanIdString::get_or_generate(req),
        }
    }
}

impl Has<XSpanIdString> for KcContext {
    fn get(&self) -> &XSpanIdString {
        &self.x_span_id
    }

    fn get_mut(&mut self) -> &mut XSpanIdString {
        &mut self.x_span_id
    }

    fn set(&mut self, value: XSpanIdString) {
        self.x_span_id = value;
    }
}

#[derive(Debug, Clone)]
pub struct AuthContext {
    x_span_id: XSpanIdString,
    authorization: Option<Authorization>,
}

impl AuthContext {
    pub fn from_request<B>(req: &Request<B>, static_tokens: &[String]) -> Self {
        let x_span_id = XSpanIdString::get_or_generate(req);
        let authorization = validate_static_bearer(req, static_tokens).then(dummy_authorization);

        Self {
            x_span_id,
            authorization,
        }
    }
}

impl Has<XSpanIdString> for AuthContext {
    fn get(&self) -> &XSpanIdString {
        &self.x_span_id
    }

    fn get_mut(&mut self) -> &mut XSpanIdString {
        &mut self.x_span_id
    }

    fn set(&mut self, value: XSpanIdString) {
        self.x_span_id = value;
    }
}

impl Has<Option<Authorization>> for AuthContext {
    fn get(&self) -> &Option<Authorization> {
        &self.authorization
    }

    fn get_mut(&mut self) -> &mut Option<Authorization> {
        &mut self.authorization
    }

    fn set(&mut self, value: Option<Authorization>) {
        self.authorization = value;
    }
}

fn dummy_authorization() -> Authorization {
    Authorization {
        subject: "static-bearer".to_owned(),
        scopes: Scopes::Some(BTreeSet::new()),
        issuer: None,
    }
}

fn validate_static_bearer<B>(req: &Request<B>, static_tokens: &[String]) -> bool {
    if static_tokens.is_empty() {
        return true;
    }

    let Some(header) = req.headers().get(http::header::AUTHORIZATION) else {
        return false;
    };
    let Ok(value) = header.to_str() else {
        return false;
    };
    let Some(token) = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")) else {
        return false;
    };

    static_tokens.iter().any(|t| t == token)
}

