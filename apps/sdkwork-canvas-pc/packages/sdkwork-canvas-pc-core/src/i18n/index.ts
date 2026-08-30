import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import LanguageDetector from 'i18next-browser-languagedetector';

import enCanvasJson from './en-US/canvas/canvas/canvas.json';
import zhCanvasJson from './zh-CN/canvas/canvas/canvas.json';
import enCommon from './en-US/canvas/common/common.json';
import zhCommon from './zh-CN/canvas/common/common.json';
import enEditor from './en-US/canvas/editor/editor.json';
import zhEditor from './zh-CN/canvas/editor/editor.json';
import enPublish from './en-US/canvas/publish/publish.json';
import zhPublish from './zh-CN/canvas/publish/publish.json';
import enOutline from './en-US/canvas/outline/outline.json';
import zhOutline from './zh-CN/canvas/outline/outline.json';
import enTemplates from './en-US/canvas/templates/templates.json';
import zhTemplates from './zh-CN/canvas/templates/templates.json';

const resources = {
  en: {
    canvas: enCanvasJson,
    common: enCommon,
    editor: enEditor,
    publish: enPublish,
    outline: enOutline,
    templates: enTemplates,
  },
  zh: {
    canvas: zhCanvasJson,
    common: zhCommon,
    editor: zhEditor,
    publish: zhPublish,
    outline: zhOutline,
    templates: zhTemplates,
  },
};

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: 'en',
    defaultNS: 'common',
    interpolation: {
      escapeValue: false,
    },
  });

export default i18n;
