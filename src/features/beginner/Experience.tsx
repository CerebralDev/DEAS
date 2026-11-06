import { useEffect, useRef } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import './beginner.css';
import './beginner-v2.css';

export type Experience = 'unset' | 'beginner' | 'advanced';
export type BeginnerPage = 'generate' | 'models' | 'collection';

export function ExperienceSwitch({value,onChange}:{value:Experience;onChange:(value:Experience)=>void}) {
 const {t}=useI18n();
 return <div className="experience-switch" role="group" aria-label={t('beginner.experience')}>
  {(['beginner','advanced'] as const).map(id=><button key={id} type="button" aria-pressed={value===id} onClick={()=>onChange(id)}>{t(`beginner.${id}`)}</button>)}
 </div>;
}

export function ExperienceWelcome({onChoose}:{onChoose:(value:Experience)=>void}) {
 const {t}=useI18n();const ref=useRef<HTMLDialogElement>(null);
 useEffect(()=>{const dialog=ref.current;if(!dialog)return;
  if(typeof dialog.showModal==='function')dialog.showModal();else dialog.setAttribute('open','');
  return()=>{if(typeof dialog.close==='function')dialog.close();};
 },[]);
 return <dialog className="experience-welcome" ref={ref} aria-labelledby="experience-title" onCancel={e=>e.preventDefault()}>
  <span className="beginner-wordmark">{"DEAS"} <small>{"Dark Engine AI Studio"}</small></span>
  <p className="eyebrow">{t('beginner.welcome')}</p><h1 id="experience-title">{t("beginner.youAre")}</h1>
  <button type="button" autoFocus className="experience-choice" onClick={()=>onChoose('beginner')}><span aria-hidden="true">✧</span><span><strong>{t("beginner.firstBeginner")}</strong><small>{t('beginner.beginnerHint')}</small></span><span aria-hidden="true">→</span></button>
  <button type="button" className="experience-choice" onClick={()=>onChoose('advanced')}><span aria-hidden="true">☷</span><span><strong>{t("beginner.firstAdvanced")}</strong><small>{t('beginner.advancedHint')}</small></span><span aria-hidden="true">→</span></button>
  <p className="field-help">{t('beginner.switchHint')}</p>
 </dialog>;
}
