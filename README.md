# vohiyo
yet another twitch chat client

#
Two environmental variables are currently required:


- `TWITCH_NAME`

This is the name associated with a generated oauth token

--- 

- `TWITCH_OAUTH`

This is the generated oauth token

#### NOTE
It must have atleast these token scopes:
* `chat:edit`
* `chat:read`


See:
<https://dev.twitch.tv/docs/irc/authenticate-bot/> for information about tokens and scopes.
