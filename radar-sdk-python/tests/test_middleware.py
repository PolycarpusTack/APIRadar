"""Tests for radar_monitor middleware."""
import json
import threading
import time
import unittest
from unittest.mock import MagicMock, patch

from radar_monitor.middleware import RadarBatcher, RadarMiddleware


class TestRadarBatcher(unittest.TestCase):
    def _make_batcher(self, max_batch=100, flush_interval=9999.0):
        return RadarBatcher(
            radar_url="http://localhost:8080",
            consumer_id="c1",
            service_id="s1",
            flush_interval=flush_interval,
            max_batch=max_batch,
        )

    def tearDown(self):
        # Each test that creates a batcher should call destroy(); this is a
        # safety net only.
        pass

    def test_push_queues_event(self):
        b = self._make_batcher()
        b.push("GET /users")
        b.push("POST /orders")
        with b._lock:
            self.assertEqual(len(b._queue), 2)
        b.destroy()

    def test_push_includes_all_fields(self):
        b = self._make_batcher()
        b.push("GET /items", "item.price")
        with b._lock:
            evt = b._queue[0]
        self.assertEqual(evt["consumer_id"], "c1")
        self.assertEqual(evt["service_id"], "s1")
        self.assertEqual(evt["operation"], "GET /items")
        self.assertEqual(evt["field_path"], "item.price")
        b.destroy()

    def test_back_pressure_drops_beyond_double_max(self):
        b = self._make_batcher(max_batch=2)
        for i in range(10):
            b.push(f"GET /op{i}")
        with b._lock:
            # Queue is capped at max_batch * 2 = 4; auto-flush fires at max_batch,
            # so real queue length depends on timing — just verify it never exceeds cap.
            self.assertLessEqual(len(b._queue), b._max_batch * 2)
        b.destroy()

    def test_auto_flush_when_max_batch_reached(self):
        with patch("urllib.request.urlopen") as mock_open:
            mock_open.return_value = MagicMock()
            b = self._make_batcher(max_batch=1)
            b.push("GET /a")
            # First push triggers flush because len(queue) >= max_batch
            b.push("GET /b")
            # urlopen was called at least once for the first flush
            self.assertTrue(mock_open.called)
            b.destroy()

    def test_flush_sends_json_body(self):
        with patch("urllib.request.urlopen") as mock_open:
            mock_open.return_value = MagicMock()
            b = self._make_batcher()
            b.push("DELETE /resource", "resource.id")
            b.flush()
            # Grab the Request object passed to urlopen
            call_args = mock_open.call_args
            req = call_args[0][0]
            payload = json.loads(req.data.decode())
            self.assertIsInstance(payload, list)
            self.assertEqual(len(payload), 1)
            self.assertEqual(payload[0]["operation"], "DELETE /resource")
            self.assertEqual(payload[0]["field_path"], "resource.id")
        b.destroy()

    def test_flush_sends_bearer_token(self):
        with patch("urllib.request.urlopen") as mock_open:
            mock_open.return_value = MagicMock()
            b = RadarBatcher(
                radar_url="http://localhost:8080",
                consumer_id="c1",
                service_id="s1",
                token="secret",
                flush_interval=9999.0,
                max_batch=100,
            )
            b.push("GET /secure")
            b.flush()
            req = mock_open.call_args[0][0]
            self.assertEqual(req.get_header("Authorization"), "Bearer secret")
        b.destroy()

    def test_flush_no_token_omits_auth_header(self):
        with patch("urllib.request.urlopen") as mock_open:
            mock_open.return_value = MagicMock()
            b = self._make_batcher()
            b.push("GET /open")
            b.flush()
            req = mock_open.call_args[0][0]
            self.assertIsNone(req.get_header("Authorization"))
        b.destroy()

    def test_flush_network_error_does_not_raise(self):
        from urllib.error import URLError
        with patch("urllib.request.urlopen", side_effect=URLError("connection refused")):
            b = self._make_batcher()
            b.push("GET /x")
            b.flush()  # must not raise
        b.destroy()

    def test_flush_empty_queue_is_noop(self):
        with patch("urllib.request.urlopen") as mock_open:
            b = self._make_batcher()
            b.flush()
            mock_open.assert_not_called()
        b.destroy()

    def test_destroy_cancels_timer(self):
        b = self._make_batcher(flush_interval=9999.0)
        b.destroy()
        # threading.Timer.cancel() sets the internal finished event synchronously.
        # Checking is_alive() is racy; check the cancellation flag instead.
        self.assertTrue(b._timer.finished.is_set())


class TestRadarMiddleware(unittest.IsolatedAsyncioTestCase):
    async def _call_middleware(self, path: str, method: str = "GET", route_path: str | None = None):
        """Helper: drive RadarMiddleware with a minimal ASGI scope."""
        recorded: list[dict] = []

        async def fake_app(scope, receive, send):
            pass

        middleware = RadarMiddleware(
            app=fake_app,
            radar_url="http://localhost:8080",
            consumer_id="c1",
            service_id="s1",
            flush_interval=9999.0,
        )
        # Patch the batcher.push so we can inspect calls without network I/O.
        calls: list[tuple] = []
        middleware._batcher.push = lambda op, fp="": calls.append((op, fp))

        scope: dict = {"type": "http", "method": method, "path": path}
        if route_path is not None:
            route_mock = MagicMock()
            route_mock.path = route_path
            scope["route"] = route_mock

        await middleware(scope, None, None)
        middleware._batcher.destroy = lambda: None  # suppress timer cancel noise
        return calls

    async def test_records_operation_for_http_request(self):
        calls = await self._call_middleware("/users/123", method="GET")
        self.assertEqual(len(calls), 1)
        self.assertIn("GET", calls[0][0])

    async def test_uses_route_pattern_when_available(self):
        calls = await self._call_middleware("/users/123", method="GET", route_path="/users/{id}")
        self.assertEqual(calls[0][0], "GET /users/{id}")

    async def test_falls_back_to_raw_path_without_route(self):
        calls = await self._call_middleware("/users/123", method="GET")
        self.assertEqual(calls[0][0], "GET /users/123")

    async def test_excludes_health_path(self):
        calls = await self._call_middleware("/health", method="GET")
        self.assertEqual(calls, [])

    async def test_excludes_metrics_path(self):
        calls = await self._call_middleware("/metrics", method="GET")
        self.assertEqual(calls, [])

    async def test_excludes_readyz(self):
        calls = await self._call_middleware("/readyz", method="GET")
        self.assertEqual(calls, [])

    async def test_excludes_livez(self):
        calls = await self._call_middleware("/livez", method="GET")
        self.assertEqual(calls, [])

    async def test_passes_through_non_http_scope(self):
        calls: list = []

        async def fake_app(scope, receive, send):
            calls.append(scope)

        middleware = RadarMiddleware(
            app=fake_app,
            radar_url="http://localhost:8080",
            consumer_id="c1",
            service_id="s1",
            flush_interval=9999.0,
        )
        batcher_calls: list = []
        middleware._batcher.push = lambda op, fp="": batcher_calls.append(op)

        await middleware({"type": "websocket", "path": "/ws"}, None, None)
        self.assertEqual(len(calls), 1)
        self.assertEqual(batcher_calls, [])


if __name__ == "__main__":
    unittest.main()
