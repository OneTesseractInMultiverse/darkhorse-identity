import type { Kind } from './catalog';

export const descriptions: Record<Kind, string> = {
	applications:
		'Register the applications that trust Darkhorse for sign-in. Open an application to configure its OIDC clients and access policies.',
	clients:
		'Configure how an application signs in with OpenID Connect (OIDC). Each client has its own client ID, secret, callback URLs and token settings.',
	resources:
		'Define protected APIs and the capabilities they expose. Each resource gets a stable audience that identifies the API an access token is intended for.',
	scopes:
		'Define named limits on the capabilities a client can request for a resource. A scope never gives a user permissions they do not already have.',
	roles:
		'Group capabilities into assignable roles, such as Reader or Administrator. Bind a role to an application before assigning it through the user directory.',
	capabilities:
		'Define individual permissions, such as invoices.read. Their keys and meanings are permanent; bind them explicitly to the applications that use them.'
};

export const nameHelp: Record<Kind, string> = {
	applications:
		'A recognizable display name, such as Customer portal. This is separate from the OIDC client registration.',
	clients:
		'A display name for this integration, such as Customer portal — production. Darkhorse generates the client ID and one-time secret when you create it.',
	resources:
		'A name for the protected API, such as Orders API. Its name and generated audience cannot be changed after creation.',
	scopes:
		'A permanent scope name, such as orders.read. Use a single value without spaces. Built-in identity scopes such as openid are reserved.',
	roles:
		'A permanent label for a group of permissions, such as Billing reader. Add capabilities after creating the role.',
	capabilities:
		'A permanent permission key, such as invoices.read. Choose a precise action that your application can enforce.'
};
