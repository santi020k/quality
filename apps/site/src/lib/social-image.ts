const trimOuterSlashes = (value: string) => value.replace(/^\/+|\/+$/g, '');

const safeDecodeURIComponent = (value: string) => {
	try {
		return decodeURIComponent(value);
	} catch {
		return value;
	}
};

export const getSocialImageSlug = (pathname: string) =>
	trimOuterSlashes(pathname)
		.split('/')
		.filter(Boolean)
		.map((segment) => encodeURIComponent(safeDecodeURIComponent(segment)).replaceAll('%', '~'))
		.join('--');

export const getSocialImagePath = (pathname: string) => `/og/pages/${getSocialImageSlug(pathname) || 'index'}.webp`;
