export class LatestRequest {
  private revision=0;
  begin(){return ++this.revision}
  isCurrent(revision:number){return this.revision===revision}
  cancel(){++this.revision}
}
