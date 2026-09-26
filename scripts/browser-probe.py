"""Run inside an isolated test browser; never inspect a personal profile."""
import asyncio
import json
import sys
import urllib.request
import websockets

async def main():
    targets = json.load(urllib.request.urlopen('http://127.0.0.1:9222/json/list'))
    target = next(t for t in targets if t['type'] == 'page')
    async with websockets.connect(target['webSocketDebuggerUrl'], origin='http://localhost') as socket:
        sequence = 0
        async def command(method, params=None):
            nonlocal sequence
            sequence += 1
            await socket.send(json.dumps({'id': sequence, 'method': method, 'params': params or {}}))
            while True:
                reply = json.loads(await socket.recv())
                if reply.get('id') == sequence:
                    if 'error' in reply:
                        raise RuntimeError(reply['error'])
                    return reply.get('result', {})
        action = sys.argv[1]
        if action == 'sandbox':
            await command('Page.navigate', {'url': 'chrome://sandbox/'})
            await asyncio.sleep(2)
            result = await command('Runtime.evaluate', {'expression': 'document.body.innerText', 'returnByValue': True})
            print(result['result']['value'])
        elif action == 'seed':
            await command('Network.setCookie', {'name': 'regionbox_test', 'value': 'persisted', 'url': 'https://example.com/', 'secure': True, 'httpOnly': True, 'expires': 1900000000})
            try:
                await command('Browser.close')
            except websockets.ConnectionClosed:
                pass
        elif action == 'cookies':
            result = await command('Network.getCookies', {'urls': ['https://example.com/']})
            print(json.dumps([{'name': c['name'], 'value': c['value']} for c in result['cookies'] if c['name'] == 'regionbox_test']))
        elif action == 'navigate':
            await command('Page.navigate', {'url': 'https://example.com/'})
            await asyncio.sleep(3)
            result = await command('Runtime.evaluate', {'expression': 'document.title', 'returnByValue': True})
            print(result['result'].get('value', ''))

asyncio.run(main())
