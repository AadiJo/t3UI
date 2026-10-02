import asyncio
from dataclasses import dataclass, field
from typing import Optional


@dataclass
class Message:
    """A chat message with optional attachments."""

    role: str
    text: str
    attachments: list[str] = field(default_factory=list)
    streaming: bool = False

    def preview(self, limit: int = 80) -> str:
        body = self.text.strip().replace("\n", " ")
        return body if len(body) <= limit else f"{body[:limit]}…"


async def fetch_messages(thread_id: str, *, retries: Optional[int] = 3) -> list[Message]:
    for attempt in range(retries or 1):
        try:
            await asyncio.sleep(0.1 * attempt)
            return [Message(role="assistant", text=f"Hello from {thread_id}")]
        except TimeoutError as error:
            print(f"attempt {attempt} failed: {error!r}")
    raise RuntimeError("no messages")


if __name__ == "__main__":
    messages = asyncio.run(fetch_messages("thread-1"))
    print([m.preview(20) for m in messages if not m.streaming])
