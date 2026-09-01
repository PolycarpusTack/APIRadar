"""Radar Monitor middleware for ASGI frameworks (FastAPI, Starlette)."""
from __future__ import annotations

import json
import queue
import threading
import time
import urllib.request
from typing import Callable, Optional, Sequence
from urllib.error import URLError


class RadarBatcher:
    """Thread-safe batch queue that flushes usage events to radar-api.

    Network I/O never runs on the calling thread: push()/flush() only swap
    the drained batch onto a send queue under the lock; a single daemon
    worker thread performs the actual HTTP POSTs. The middleware calls
    push() from the async request path, so a synchronous urlopen there
    would stall the event loop for up to the 3s socket timeout.
    """

    def __init__(
        self,
        radar_url: str,
        consumer_id: str,
        service_id: str,
        token: str | None = None,
        flush_interval: float = 5.0,
        max_batch: int = 100,
    ) -> None:
        self._url = radar_url.rstrip("/") + "/v1/usage/events"
        self._consumer_id = consumer_id
        self._service_id = service_id
        self._token = token
        self._max_batch = max_batch
        self._queue: list[dict] = []
        self._lock = threading.Lock()
        self._send_queue: queue.Queue[Optional[list]] = queue.Queue()
        self._worker = threading.Thread(
            target=self._send_loop, name="radar-batcher-send", daemon=True
        )
        self._worker.start()
        self._timer = threading.Timer(flush_interval, self._scheduled_flush)
        self._timer.daemon = True
        self._timer.start()
        self._flush_interval = flush_interval

    def push(self, operation: str, field_path: str = "") -> None:
        with self._lock:
            if len(self._queue) >= self._max_batch * 2:
                return  # back-pressure: drop silently
            self._queue.append({
                "consumer_id": self._consumer_id,
                "service_id": self._service_id,
                "operation": operation,
                "field_path": field_path,
            })
            if len(self._queue) >= self._max_batch:
                self._flush_locked()

    def flush(self) -> None:
        with self._lock:
            self._flush_locked()

    def _flush_locked(self) -> None:
        """Drain up to max_batch events and hand them to the send worker.

        Caller must hold the lock. No network I/O happens here — the lock
        is held only for the queue swap.
        """
        if not self._queue:
            return
        batch = self._queue[:self._max_batch]
        self._queue = self._queue[self._max_batch:]
        self._send_queue.put(batch)

    def _send_loop(self) -> None:
        while True:
            batch = self._send_queue.get()
            try:
                if batch is None:
                    return  # destroy() sentinel
                self._send(batch)
            except Exception:
                pass  # fire-and-forget; the worker must never die
            finally:
                self._send_queue.task_done()

    def _send(self, batch: list) -> None:
        body = json.dumps(batch).encode()
        headers = {"Content-Type": "application/json"}
        if self._token:
            headers["Authorization"] = f"Bearer {self._token}"
        req = urllib.request.Request(self._url, data=body, headers=headers, method="POST")
        try:
            urllib.request.urlopen(req, timeout=3)
        except (URLError, OSError):
            pass  # fire-and-forget; never crash on network errors

    def _scheduled_flush(self) -> None:
        self.flush()
        # Reschedule
        self._timer = threading.Timer(self._flush_interval, self._scheduled_flush)
        self._timer.daemon = True
        self._timer.start()

    def destroy(self) -> None:
        self._timer.cancel()
        self.flush()
        # Stop the worker after it drains what is already enqueued (FIFO:
        # the sentinel lands behind the final batch). Bounded wait — this
        # stays best-effort fire-and-forget.
        self._send_queue.put(None)
        self._worker.join(timeout=5.0)


class RadarMiddleware:
    """ASGI middleware that reports operation-level usage events to Radar Monitor.

    Compatible with FastAPI, Starlette, and any other ASGI framework.

    Usage::

        from fastapi import FastAPI
        from radar_monitor import RadarMiddleware

        app = FastAPI()
        app.add_middleware(
            RadarMiddleware,
            radar_url="http://radar-api:8080",
            consumer_id="my-consumer-id",
            service_id="my-service-id",
            token="optional-token",
        )
    """

    def __init__(
        self,
        app: Callable,
        radar_url: str,
        consumer_id: str,
        service_id: str,
        token: str | None = None,
        flush_interval: float = 5.0,
        max_batch: int = 100,
        exclude_paths: Sequence[str] = ("/health", "/metrics", "/readyz", "/livez"),
    ) -> None:
        self.app = app
        self._batcher = RadarBatcher(
            radar_url=radar_url,
            consumer_id=consumer_id,
            service_id=service_id,
            token=token,
            flush_interval=flush_interval,
            max_batch=max_batch,
        )
        self._exclude = set(exclude_paths)

    async def __call__(self, scope, receive, send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        path = scope.get("path", "")
        if path in self._exclude:
            await self.app(scope, receive, send)
            return

        method = scope.get("method", "GET").upper()

        await self.app(scope, receive, send)

        # Record AFTER the app call: Starlette/FastAPI set scope["route"]
        # during routing (inside the call above), mutating the scope dict in
        # place. Reading it earlier would only ever see the raw path and
        # record per-ID operations (/users/123) instead of the route pattern.
        route = scope.get("route", None)
        route_path = route.path if route and hasattr(route, "path") else path
        self._batcher.push(f"{method} {route_path}")
