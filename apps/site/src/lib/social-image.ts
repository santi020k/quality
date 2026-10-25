import { getGeneratedSocialImagePath } from './generated-social-image'

import { pathnameOutput } from '@santi020k/og';

export const getSocialImagePath = (pathname: string) => getGeneratedSocialImagePath(pathname) ?? `/og/pages/${pathnameOutput(pathname)}`;
