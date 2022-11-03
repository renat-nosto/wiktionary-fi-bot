token=$0
cloudflare_user=$1
curl "https://api.telegram.org/bot$token/setWebhook?url=https://fiwiki2.$cloudflare_user.workers.dev/bot_endpoint"