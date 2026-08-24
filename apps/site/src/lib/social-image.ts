import { pathnameOutput } from '@santi020k/og';

export const getSocialImagePath = (pathname: string) => `/og/pages/${pathnameOutput(pathname)}`;
