import {useThemeState} from '../../lib/theme';
import {MouseMischief} from './MouseMischief';
import {FrenchWineProgress} from './FrenchWineProgress';
import {ParisScene} from './ParisScene';
import './french.css';
import './paris.css';
import './theme-packs.css';

export function ThemeAmbient(){const {active}=useThemeState();return active?.effects.mouse?<MouseMischief key={active.id} mouse={active.assets.mouse} cheese={active.assets.cheese}/>:null;}
export function ThemeScene(){const {active}=useThemeState();return active?.presentation==='paris'?<ParisScene lamp={active.assets['street-lamp']} eiffel={active.assets.eiffel}/>:null;}
export function ThemeProgress({active,progress,label}:{active:boolean;progress?:{value:number;max:number};label:string}){const state=useThemeState();return state.active?.effects.wine?<FrenchWineProgress active={active} progress={progress} label={label} wineFull={state.active.assets.wine} wineEmpty={state.active.assets['wine-empty']}/>:null;}

