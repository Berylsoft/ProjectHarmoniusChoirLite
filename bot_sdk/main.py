import asyncio
import signal

from bot import NotifyReview, PhclBot


async def main():
    async def handle_notify(msg: NotifyReview):
        print(msg)

    async def handle_closed():
        print("closed")

    bot = PhclBot(handle_notify, handle_closed)
    print(bot.sign_login("2", True))
    print(bot.sign_login_token("1", True))
    await bot.notify_connect()

    sigint = asyncio.Event()
    asyncio.get_running_loop()\
        .add_signal_handler(signal.SIGINT, sigint.set)

    await sigint.wait()

    print("closing")
    await bot.notify_close()


if __name__ == "__main__":
    asyncio.run(main())
