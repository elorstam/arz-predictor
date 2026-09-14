export class VersionedCache<T> {
  private entries = new Map<string, { version: string; value: Promise<T> }>();
  get(key: string, version: string, load: () => Promise<T>): Promise<T> {
    const entry=this.entries.get(key);
    if(entry?.version===version)return entry.value;
    const value=Promise.resolve().then(load);
    this.entries.set(key,{version,value});
    value.catch(()=>{if(this.entries.get(key)?.value===value)this.entries.delete(key)});
    if(this.entries.size>24)this.entries.delete(this.entries.keys().next().value!);
    return value;
  }
}
