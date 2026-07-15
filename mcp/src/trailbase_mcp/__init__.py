"""FastMCP integration for TrailBase."""

from .client import TrailBaseClient
from .server import mcp

__all__ = ["TrailBaseClient", "mcp"]
