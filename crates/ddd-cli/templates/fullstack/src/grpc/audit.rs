#![allow(unused_imports)]
#![allow(dead_code)]

use std::collections::VecDeque;
use std::pin::Pin;

use futures::Stream;
use tonic::{Request, Response, Status};
use wasi_auth::authentication::jwt;

use super::admin_proto::admin_service_server::{AdminService, AdminServiceServer};
use super::audit_proto::audit_service_server::{AuditService, AuditServiceServer};
use super::auth_proto::auth_service_server::{AuthService, AuthServiceServer};
use super::authorization_proto::authorization_service_server::{
    AuthorizationService, AuthorizationServiceServer,
};
use super::organization_proto::organization_service_server::{
    OrganizationService, OrganizationServiceServer,
};
use super::*;

#[tonic::async_trait]

impl AuditService for AuditGrpcService {
    async fn list_audit_events(
        &self,
        request: Request<audit_proto::ListAuditEventsRequest>,
    ) -> Result<Response<audit_proto::AuditEventListResponse>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        let response = crate::application::list_audit_events(
            empty_to_option(request.organization_id),
            request.after_cursor,
            usize::try_from(request.limit.clamp(1, 100)).unwrap_or(100),
            auth,
        )
        .await
        .map_err(|error| status_from_app_error("Audit.ListAuditEvents", error))?;
        Ok(Response::new(response.into()))
    }

    type WatchAuditEventsStream =
        Pin<Box<dyn Stream<Item = Result<audit_proto::AuditEvent, Status>> + Send>>;

    async fn watch_audit_events(
        &self,
        request: Request<audit_proto::WatchAuditEventsRequest>,
    ) -> Result<Response<Self::WatchAuditEventsStream>, Status> {
        let auth = request_auth(&request);
        let request = request.into_inner();
        crate::application::list_audit_events(
            empty_to_option(request.organization_id.clone()),
            request.after_cursor,
            1,
            auth.clone(),
        )
        .await
        .map_err(|error| status_from_app_error("Audit.WatchAuditEvents.authorize", error))?;
        let state = AuditWatchState {
            organization_id: empty_to_option(request.organization_id),
            cursor: request.after_cursor,
            buffered: VecDeque::new(),
            auth,
            started_at: wasip3::clocks::monotonic_clock::now(),
            terminated: false,
        };
        let stream = futures::stream::unfold(state, |mut state| async move {
            if state.terminated
                || wasip3::clocks::monotonic_clock::now().saturating_sub(state.started_at)
                    >= AUDIT_STREAM_WINDOW_NANOS
            {
                return None;
            }
            loop {
                if let Some(event) = state.buffered.pop_front() {
                    if let Err(error) = crate::application::list_audit_events(
                        state.organization_id.clone(),
                        state.cursor,
                        1,
                        state.auth.for_revalidation(),
                    )
                    .await
                    {
                        state.terminated = true;
                        return Some((
                            Err(status_from_app_error(
                                "Audit.WatchAuditEvents.reauthorize",
                                error,
                            )),
                            state,
                        ));
                    }
                    state.cursor = event.sequence;
                    return Some((Ok(event.into()), state));
                }
                match crate::application::list_audit_events(
                    state.organization_id.clone(),
                    state.cursor,
                    100,
                    state.auth.for_revalidation(),
                )
                .await
                {
                    Ok(response) if response.events.is_empty() => {
                        wasip3::clocks::monotonic_clock::wait_for(250_000_000).await;
                        if wasip3::clocks::monotonic_clock::now().saturating_sub(state.started_at)
                            >= AUDIT_STREAM_WINDOW_NANOS
                        {
                            return None;
                        }
                    }
                    Ok(response) => state.buffered.extend(response.events),
                    Err(error) => {
                        state.terminated = true;
                        return Some((
                            Err(status_from_app_error("Audit.WatchAuditEvents.poll", error)),
                            state,
                        ));
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

pub(crate) struct AuditWatchState {
    organization_id: Option<String>,
    cursor: u64,
    buffered: VecDeque<crate::contracts::AuditEventSummary>,
    auth: crate::application::RequestAuth,
    started_at: u64,
    terminated: bool,
}

