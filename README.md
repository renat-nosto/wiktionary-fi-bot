# wiktionary-fi-bot
Telegram bot to fetch wiktionary translations 

Designed to be deployed to herouku, uses webhooks to receive messages from telegram
Uses ENV variables:
  * `BOT_TOKEN` - token bot
  * `ORIGIN` - protocol, host, port for setting up webhook to telegram
  * `SECRET_PATH` - path to register webhook
  * `PORT` - port to listen to for webhook server

