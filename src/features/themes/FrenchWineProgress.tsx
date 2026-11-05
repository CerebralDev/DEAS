import {useI18n} from '../../i18n/I18nProvider';
import { useId } from 'react';

/** A presentation of native progress, never a timer or an estimated percentage. */
export function FrenchWineProgress({active,progress,label,wineFull,wineEmpty}:{active:boolean;progress?:{value:number;max:number};label:string;wineFull:string;wineEmpty:string}) {
 const {t}=useI18n();
 const clip=useId().replace(/:/g,'');
 if(!active)return null;
 const known=!!progress&&Number.isFinite(progress.value)&&Number.isFinite(progress.max)&&progress.max>0;
 const percent=known?Math.round(Math.max(0,Math.min(1,progress!.value/progress!.max))*100):undefined;
 const remaining=percent===undefined?100:100-percent;
 return <div className={`french-wine${known?'':' is-indeterminate'}`} data-french-wine role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}>
  <svg viewBox="0 0 36 36" aria-hidden="true">
   <defs><clipPath id={clip}><rect data-wine-liquid x="0" y={18-remaining*.1} width="36" height={remaining*.1}/></clipPath></defs>
   <image href={wineEmpty} width="36" height="36"/>
   <image href={wineFull} width="36" height="36" clipPath={`url(#${clip})`}/>
  </svg>
  <div><small>{t('theme.ui.pixelVintage')}</small><strong>{percent===undefined?t('theme.ui.wineWaiting'):`${percent}%`}</strong><span>{label}</span><em>{t('theme.ui.wineInProgress')}</em></div>
 </div>;
}
