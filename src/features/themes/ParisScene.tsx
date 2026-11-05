export function ParisScene({lamp,eiffel}:{lamp?:string;eiffel?:string}){
 return <div className="paris-scene" aria-hidden="true"><span className="paris-moon"/><div className="paris-lamp left"><i/><img src={lamp} alt=""/></div><div className="paris-lamp right"><i/><img src={lamp} alt=""/></div><img className="paris-tower" src={eiffel} alt=""/><span className="paris-river"/><span className="paris-hearts">♡</span></div>;
}
