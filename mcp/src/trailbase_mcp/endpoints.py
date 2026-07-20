from __future__ import annotations

from typing import Any
from urllib.parse import quote

TRAILBASE_API_OPERATIONS: tuple[dict[str, Any], ...] = (
    {
        "operation_id": "auth_code_to_token_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/token",
        "summary": "Exchange authorization code for auth tokens.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "change_email_confirm_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/change_email/confirm/:email_verification_code",
        "summary": "Confirm a change of email address.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "change_email_request_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/change_email/request",
        "summary": "Request an email change.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "change_password_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/change_password",
        "summary": "Request a change of password.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "create_avatar_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/avatar/",
        "summary": "Create or update the current user's avatar.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "delete_avatar_handler",
        "category": "auth",
        "method": "DELETE",
        "path": "/api/auth/v1/avatar/",
        "summary": "Delete the current user's avatar.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "delete_handler",
        "category": "auth",
        "method": "DELETE",
        "path": "/api/auth/v1/delete",
        "summary": "Delete the current user.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "get_avatar_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/avatar/:b64_user_id",
        "summary": "Get a user's avatar.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": False,
    },
    {
        "operation_id": "login_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/login",
        "summary": "Log in users by email and password.",
        "mcp_support": "built-in MCP sidecar login, call_trailbase_api_operation, or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "login_mfa_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/login_mfa",
        "summary": "Log in users with an MFA token.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "login_otp_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/otp/login",
        "summary": "Log in with an OTP code.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "login_status_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/status",
        "summary": "Check login status.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": False,
    },
    {
        "operation_id": "logout_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/logout",
        "summary": "Log out the current user and delete all pending sessions.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "post_logout_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/logout",
        "summary": "Log out the session for a refresh token.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "refresh_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/refresh",
        "summary": "Refresh auth tokens given a refresh token.",
        "mcp_support": "built-in MCP sidecar refresh, call_trailbase_api_operation, or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "register_totp_confirm_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/totp/confirm",
        "summary": "Verify the current user's TOTP.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "register_totp_request_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/totp/register",
        "summary": "Register the current user for TOTP.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "register_user_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/register",
        "summary": "Register a new user with email and password.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "request_email_verification_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/verify_email/trigger",
        "summary": "Request a new email verification email.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "request_otp_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/otp/request",
        "summary": "Request an OTP code.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "reset_password_request_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/reset_password/request",
        "summary": "Request a password reset.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "reset_password_update_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/reset_password/update",
        "summary": "Set a new password after a reset request.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "unregister_totp_handler",
        "category": "auth",
        "method": "POST",
        "path": "/api/auth/v1/totp/unregister",
        "summary": "Unregister TOTP for the current user.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "verify_email_handler",
        "category": "auth",
        "method": "GET",
        "path": "/api/auth/v1/verify_email/confirm/:email_verification_code",
        "summary": "Confirm an email verification code.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "callback_from_external_auth_provider",
        "category": "oauth",
        "method": "GET",
        "path": "/api/auth/v1/oauth/{provider}/callback",
        "summary": "Handle an external OAuth provider callback.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": True,
    },
    {
        "operation_id": "list_configured_providers_handler",
        "category": "oauth",
        "method": "GET",
        "path": "/api/auth/v1/oauth/providers",
        "summary": "List configured OAuth providers.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": False,
    },
    {
        "operation_id": "login_with_external_auth_provider",
        "category": "oauth",
        "method": "GET",
        "path": "/api/auth/v1/oauth/{provider}/login",
        "summary": "Start login through an external OAuth provider.",
        "mcp_support": "call_trailbase_api_operation or trailbase_request",
        "requires_write_permission": False,
    },
    {
        "operation_id": "add_subscription_sse_handler",
        "category": "records",
        "method": "GET",
        "path": "/api/records/v1/{name}/subscribe/{record}",
        "summary": "Start streaming record changes via SSE/WebSocket.",
        "mcp_support": "catalog only; long-running SSE is not proxied by this request/response MCP tool",
        "requires_write_permission": False,
        "streaming": True,
    },
    {
        "operation_id": "create_record_handler",
        "category": "records",
        "method": "POST",
        "path": "/api/records/v1/{name}",
        "summary": "Create a new record.",
        "mcp_support": "create_record, create_record_with_file_uploads, create_record_multipart, or call_trailbase_api_operation",
        "requires_write_permission": True,
    },
    {
        "operation_id": "delete_record_handler",
        "category": "records",
        "method": "DELETE",
        "path": "/api/records/v1/{name}/{record}",
        "summary": "Delete a record.",
        "mcp_support": "delete_record or call_trailbase_api_operation",
        "requires_write_permission": True,
    },
    {
        "operation_id": "get_uploaded_file_from_record_handler",
        "category": "records",
        "method": "GET",
        "path": "/api/records/v1/{name}/{record}/file/{column_name}",
        "summary": "Read a file associated with a record.",
        "mcp_support": "download_file or call_trailbase_api_operation",
        "requires_write_permission": False,
    },
    {
        "operation_id": "get_uploaded_files_from_record_handler",
        "category": "records",
        "method": "GET",
        "path": "/api/records/v1/{name}/{record}/files/{column_name}/{file_name}",
        "summary": "Read one file from a record file-list column.",
        "mcp_support": "download_file or call_trailbase_api_operation",
        "requires_write_permission": False,
    },
    {
        "operation_id": "json_schema_handler",
        "category": "records",
        "method": "GET",
        "path": "/api/records/v1/{name}/schema",
        "summary": "Retrieve the JSON Schema for a record API.",
        "mcp_support": "get_api_json_schema or call_trailbase_api_operation",
        "requires_write_permission": False,
    },
    {
        "operation_id": "list_records_handler",
        "category": "records",
        "method": "GET",
        "path": "/api/records/v1/{name}",
        "summary": "List records matching filters.",
        "mcp_support": "list_records or call_trailbase_api_operation",
        "requires_write_permission": False,
    },
    {
        "operation_id": "read_record_handler",
        "category": "records",
        "method": "GET",
        "path": "/api/records/v1/{name}/{record}",
        "summary": "Read one record.",
        "mcp_support": "get_record or call_trailbase_api_operation",
        "requires_write_permission": False,
    },
    {
        "operation_id": "update_record_handler",
        "category": "records",
        "method": "PATCH",
        "path": "/api/records/v1/{name}/{record}",
        "summary": "Update an existing record.",
        "mcp_support": "update_record or call_trailbase_api_operation",
        "requires_write_permission": True,
    },
)


def list_api_operations(category: str | None = None) -> list[dict[str, Any]]:
    if category is None:
        return [dict(operation) for operation in TRAILBASE_API_OPERATIONS]
    normalized = category.lower()
    return [
        dict(operation)
        for operation in TRAILBASE_API_OPERATIONS
        if operation["category"] == normalized
    ]


def get_api_operation(operation_id: str) -> dict[str, Any]:
    for operation in TRAILBASE_API_OPERATIONS:
        if operation["operation_id"] == operation_id:
            return dict(operation)
    raise ValueError(f"Unknown TrailBase API operation: {operation_id}")


def render_operation_path(
    operation: dict[str, Any],
    path_params: dict[str, Any] | None = None,
) -> str:
    path = operation["path"]
    params = path_params or {}

    for name, value in params.items():
        value = quote(str(value), safe="")
        path = path.replace(f"{{{name}}}", value)
        path = path.replace(f":{name}", value)

    if "{" in path or "}" in path or "/:" in path:
        raise ValueError(
            f"Missing path parameter for {operation['operation_id']}: {operation['path']}"
        )

    return path
