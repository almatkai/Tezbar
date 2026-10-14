# Google Calendar sign-in

Tezbar preserves the built-in Google Calendar extension's Raycast client and
`com.raycast:/oauth` redirect. On Windows, it temporarily registers that protocol
while sign-in is pending, forwards the response to the extension's loopback
listener, and restores the previous handler after completion, cancellation, or
timeout. It leaves the separate `raycast:` protocol alone.

If the browser does not return automatically, a helper page opens after 15 seconds.
Finish signing in in the Google tab and allow the browser to open Tezbar. If that
fails, paste the **complete redirect URL**, including `code` and `state`, into the
helper page. A bare code or a URL from a different attempt cannot complete sign-in.
The helper sends the URL in a POST form rather than putting it in browser history.
Each attempt expires after two minutes.

## Use your own Google Cloud project

Open Google Calendar's **Extension Preferences** in Tezbar:

1. Create an OAuth client of type **Desktop app** in Google Cloud, enable the
   Google Calendar API, and configure the consent screen/test users as needed.
2. Enter its **Google Desktop Client ID**. If the credentials include a client
   secret, enter **Google Desktop Client Secret** as well.
3. Save and run the extension again to sign in with that client.

When an override is present, Tezbar uses an ephemeral
`http://127.0.0.1:<port>/oauth/callback` redirect, PKCE, and that client's ID for
both token exchange and refresh. It does not register a Windows protocol. Tokens
are stored separately for each Desktop client. Clear both fields to return to the
built-in client. Existing built-in tokens remain available.

Desktop credentials and tokens are stored locally using the existing extension
preferences/support storage. The client secret is sent only to the token endpoint.

Google's [installed-app OAuth documentation](https://developers.google.com/identity/protocols/oauth2/native-app)
and [loopback migration guide](https://developers.google.com/identity/protocols/oauth2/resources/loopback-migration)
describe the supported client types and redirect methods. Compatibility with the
built-in Raycast client also depends on that client's Google Cloud configuration;
Tezbar cannot change it.
