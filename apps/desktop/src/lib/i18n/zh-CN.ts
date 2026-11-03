import common from './zh-CN/common.ts';
import nav from './zh-CN/nav.ts';
import home from './zh-CN/home.ts';
import clients from './zh-CN/clients.ts';
import instance from './zh-CN/instance.ts';
import mods from './zh-CN/mods.ts';
import settings from './zh-CN/settings.ts';
import account from './zh-CN/account.ts';
import welcome from './zh-CN/welcome.ts';
import errors from './zh-CN/errors.ts';

export const zhCN = { common, nav, home, clients, instance, mods, settings, account, errors, welcome };
export type Messages = typeof zhCN;
