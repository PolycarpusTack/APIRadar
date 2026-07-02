// Basename-aware detection of the public share route.
//
// The app is served at `/` in dev but under `/app/` in production (see
// vite.config base). `window.location.pathname` therefore includes the router
// basename, so a naive `startsWith('/share/')` misses `/app/share/<token>` and
// wrongly auth-gates the public share page. Strip the basename first, then test.

/**
 * Remove the router basename prefix from a pathname. Returns a path that always
 * starts with `/`. If the pathname does not start with the basename (or the
 * basename is empty), the pathname is returned unchanged (normalized to lead
 * with `/`).
 */
export function stripBasename(pathname: string, basename = ''): string {
  const base = basename.replace(/\/$/, '')
  if (base && (pathname === base || pathname.startsWith(base + '/'))) {
    const rest = pathname.slice(base.length)
    return rest.startsWith('/') ? rest : `/${rest}`
  }
  return pathname.startsWith('/') ? pathname : `/${pathname}`
}

/**
 * True when `pathname` addresses the public share route (`/share/:token`),
 * accounting for the router `basename`. Works in both dev (`/share/…`) and
 * production (`/app/share/…`).
 */
export function isSharePath(pathname: string, basename = ''): boolean {
  return stripBasename(pathname, basename).startsWith('/share/')
}
