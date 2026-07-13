from dataclasses import dataclass
from typing import Any, Callable, Coroutine, Literal, Union, cast
from cryptography.hazmat.primitives.asymmetric import ed25519
from cryptography.hazmat.primitives import serialization as crypto_ser
from datetime import datetime, timezone
import asyncio
import base64
import websockets
import cbor2


Group = Union[Literal["Choir"], Literal["Lead"], Literal["Harmony"]]

@dataclass
class ReviewResultReject:
    action: Literal["Reject"]

@dataclass
class ReviewResultPass:
    action: Literal["Pass"]
    groups: set[Group]
    ignored: bool

@dataclass
class NotifyReview:
    id: str
    result: Union[ReviewResultReject, ReviewResultPass]
    comment: Union[str, None]

NotifyCb = Callable[[NotifyReview], Coroutine[Any, Any, None]]
NotifyClosedCb = Callable[[], Coroutine[Any, Any, None]]

class PhclBot:
    key: ed25519.Ed25519PrivateKey
    login_prefix: str
    notify_prefix: str

    notify: websockets.ClientConnection
    notify_running: bool

    notify_cb: NotifyCb
    notify_closed_cb: NotifyClosedCb

    def __init__(
            self,
            notify_cb: NotifyCb,
            notify_closed_cb: NotifyClosedCb,
            host = "127.0.0.1:29701",
            use_secure = False,
            sk_path = "./sk.der"
        ) -> None:
        self.notify_cb = notify_cb
        self.notify_closed_cb =notify_closed_cb

        with open(sk_path, "rb") as f:
            sk_der = f.read()
            key = crypto_ser.load_der_private_key(sk_der, password=None)

            assert isinstance(key, ed25519.Ed25519PrivateKey), \
            "sk_path doesn't point to a valid ed25519 secret key"
            self.key = cast(ed25519.Ed25519PrivateKey, key)

        http_proto = "https" if use_secure else "http"
        ws_proto = "wss" if use_secure else "ws"

        self.login_prefix = f"{http_proto}://{host}/auth/login?token="
        self.notify_prefix = f"{ws_proto}://{host}/notify/bot?token="

    def sign_login(self, id: str, is_manager: bool) -> str:
        return f"{self.login_prefix}{self._sign_login(id, is_manager)}"

    def sign_login_token(self, id: str, is_manager: bool) -> str:
        return f"AL{self._sign_login(id, is_manager)}AL"

    async def notify_connect(self):
        self.notify = await websockets.connect(self._sign_notify())
        self.notify_running = True
        self.notify_task = asyncio.create_task(self._notify_recv())

    async def notify_close(self):
        if not self.notify_running:
            return

        self.notify_running = False

        self.notify_task.cancel()
        try:
            await self.notify_task
        except asyncio.CancelledError:
            pass

        await self.notify.close()

        await self.notify_closed_cb()

    def _sign(self, data: Any) -> str:
        cbor_data = cbor2.dumps(data)
        sig = self.key.sign(cbor_data)
        token_bytes = sig + cbor_data
        return base64.urlsafe_b64encode(token_bytes).rstrip(b"=").decode()

    def _sign_login(self, id: str, is_manager: bool) -> str:
        created_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
        token_body = {
            "id": id,
            "is_manager": is_manager,
            "created_at": created_at
        }
        token = self._sign(token_body)
        return token

    def _sign_notify(self) -> str:
        created_at = datetime.now(timezone.utc).isoformat(timespec="seconds")
        token_body = { "created_at": created_at }
        token = self._sign(token_body)
        return f"{self.notify_prefix}{token}"

    async def _notify_recv(self):
        try:
            while self.notify_running:
                msg = await self.notify.recv()
                assert isinstance(msg, bytes), "expect bytes"
                await self._notify_on_msg(cast(bytes, msg))
        except websockets.ConnectionClosed:
            pass
        finally:
            await self.notify_close()

    async def _notify_on_msg(self, msg_bytes: bytes):
        msg = cbor2.loads(msg_bytes)
        assert msg["type"] == "Review", "expect Review notification"

        data = msg["data"]
        result = data["result"]
        action = result["action"]

        if action == "Reject":
            result = ReviewResultReject(action="Reject")
        else:
            assert action == "Pass", "expect exhaustive handling"

            res_data = result["data"]
            result = ReviewResultPass(
                action="Pass",
                groups=set(res_data["groups"]),
                ignored=res_data["ignored"]
            )

        id = data["id"]
        comment = data["comment"]
        await self.notify_cb(NotifyReview(id, result, comment))

def gen_keys():
    sk = ed25519.Ed25519PrivateKey.generate()
    pk = sk.public_key()

    sk_der = sk.private_bytes(
        encoding=crypto_ser.Encoding.DER,
        format=crypto_ser.PrivateFormat.PKCS8,
        encryption_algorithm=crypto_ser.NoEncryption()
    )
    with open("sk.der", "wb") as f:
        f.write(sk_der)

    pk_der = pk.public_bytes(
        encoding=crypto_ser.Encoding.DER,
        format=crypto_ser.PublicFormat.SubjectPublicKeyInfo,
    )
    with open("pk.der", "wb") as f:
        f.write(pk_der)

if __name__ == "__main__":
    gen_keys()
