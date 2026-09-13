/*
* Copyright (c) 2026. caoccao.com Sam Cao
* All rights reserved.

* Licensed under the Apache License, Version 2.0 (the "License");
* you may not use this file except in compliance with the License.
* You may obtain a copy of the License at

* http://www.apache.org/licenses/LICENSE-2.0

* Unless required by applicable law or agreed to in writing, software
* distributed under the License is distributed on an "AS IS" BASIS,
* WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
* See the License for the specific language governing permissions and
* limitations under the License.
*/

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { Language, LANGUAGES } from '../lib/protocol';
import de from '../../locales/de.json';
import enUS from '../../locales/en-US.json';
import es from '../../locales/es.json';
import fr from '../../locales/fr.json';
import it from '../../locales/it.json';
import ja from '../../locales/ja.json';
import zhCN from '../../locales/zh-CN.json';
import zhHK from '../../locales/zh-HK.json';
import zhTW from '../../locales/zh-TW.json';

/** Resolve regional and script tags to a bundled locale, with English as fallback. */
export function resolveLanguage(language?: string): Language {
  const tag = (language ?? '').trim().replace(/_/g, '-').toLowerCase();
  const exact = LANGUAGES.find((locale) => locale.toLowerCase() === tag);
  if (exact) return exact;
  const [base, ...subtags] = tag.split('-');
  if (base === 'zh') {
    if (subtags.includes('hk') || subtags.includes('mo')) return Language.ZhHK;
    if (subtags.includes('hant') || subtags.includes('tw')) return Language.ZhTW;
    return Language.ZhCN;
  }
  return LANGUAGES.find((locale) => locale === base) ?? Language.EnUS;
}

function syncDocumentLanguage() {
  if (typeof document !== 'undefined') {
    document.documentElement.lang = i18n.resolvedLanguage ?? Language.EnUS;
  }
}

i18n.on('languageChanged', syncDocumentLanguage);

i18n.use(initReactI18next).init({
  resources: {
    de: { translation: de },
    'en-US': { translation: enUS },
    es: { translation: es },
    fr: { translation: fr },
    it: { translation: it },
    ja: { translation: ja },
    'zh-CN': { translation: zhCN },
    'zh-HK': { translation: zhHK },
    'zh-TW': { translation: zhTW },
  },
  lng: Language.EnUS,
  supportedLngs: LANGUAGES,
  load: 'currentOnly',
  fallbackLng: Language.EnUS,
  interpolation: {
    escapeValue: false,
  },
});

export function changeLanguage(language?: string) {
  return i18n.changeLanguage(resolveLanguage(language));
}

export default i18n;
