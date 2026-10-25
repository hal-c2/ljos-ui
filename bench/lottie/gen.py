#!/usr/bin/env python3
# Writes bench/lottie/mid.json, the animation bench/lottie.qml plays (python3 bench/lottie/gen.py).
# A mid-sized Lottie: 40 shape layers, keyframed transforms with easing,
# rects, ellipses, stars, paths, fills, strokes with dashes, gradients,
# trims, one precomp used 4 times. 512x512, 60 fps, 180 frames.
import json, math, sys
def k(v): return {"a":0,"k":v}
def kf(t0,v0,t1,v1):
    return {"a":1,"k":[{"t":t0,"s":v0,"o":{"x":[0.33],"y":[0]},"i":{"x":[0.67],"y":[1]}},{"t":t1,"s":v1}]}
def tr(): return {"ty":"tr","p":k([0,0]),"a":k([0,0]),"s":k([100,100]),"r":k(0),"o":k(100)}
layers=[]
for n in range(40):
    x=40+(n%8)*60; y=40+(n//8)*90
    kind=n%5
    if kind==0: geo={"ty":"rc","p":k([0,0]),"s":kf(0,[20,20],90,[50,30]),"r":k(6)}
    elif kind==1: geo={"ty":"el","p":k([0,0]),"s":kf(0,[30,30],120,[10,40])}
    elif kind==2: geo={"ty":"sr","sy":1,"p":k([0,0]),"pt":k(5),"or":k(25),"ir":k(10),"r":kf(0,0,180,360),"os":k(0),"is":k(0)}
    elif kind==3: geo={"ty":"sr","sy":2,"p":k([0,0]),"pt":k(6),"or":k(22),"r":k(0),"os":k(0)}
    else:
        pts=[[math.cos(a)*25,math.sin(a)*25] for a in [i*math.pi/4 for i in range(8)]]
        geo={"ty":"sh","ks":k({"c":True,"v":pts,"i":[[3,-3]]*8,"o":[[-3,3]]*8})}
    items=[geo]
    if n%3==0:
        items.append({"ty":"gf","t":1+(n%2),"s":k([-25,0]),"e":k([25,0]),"g":{"p":3,"k":k([0,1,0,0,0.5,0,1,0,1,0,0,1])},"o":k(100)})
    else:
        items.append({"ty":"fl","c":k([(n*37%255)/255,(n*91%255)/255,(n*53%255)/255,1]),"o":k(90)})
    items.append({"ty":"st","c":k([1,1,1,1]),"w":kf(0,[1],90,[4]),"o":k(100),"lc":2,"lj":2,
                  "d":[{"n":"d","v":k(6)},{"n":"g","v":k(3)},{"n":"o","v":kf(0,[0],180,[36])}] if n%4==0 else []})
    if n%6==0: items.append({"ty":"tm","s":k(0),"e":kf(0,[0],120,[100]),"o":k(0),"m":1})
    items.append(tr())
    layers.append({"ty":4,"ind":n+1,"ip":0,"op":180,"st":0,
        "ks":{"p":kf(0,[x,y],180,[512-x,y+20]),"a":k([0,0]),"s":kf(0,[100,100],90,[140,80]),
              "r":kf(0,0,180,90*(1 if n%2 else -1)),"o":k(100)},
        "shapes":[{"ty":"gr","it":items}]})
pre=[{"ty":4,"ind":1,"ip":0,"op":180,"st":0,"ks":{"r":kf(0,0,180,720),"p":k([0,0])},
      "shapes":[{"ty":"gr","it":[{"ty":"rc","p":k([10,0]),"s":k([20,6]),"r":k(3)},{"ty":"fl","c":k([1,0.8,0,1]),"o":k(100)},tr()]}]}]
for i in range(4):
    layers.append({"ty":0,"refId":"spin","ind":100+i,"w":64,"h":64,"ip":0,"op":180,"st":-i*10,
                   "ks":{"p":k([64+i*120,470]),"a":k([0,0])}})
doc={"v":"5.7.0","fr":60,"ip":0,"op":180,"w":512,"h":512,"layers":layers[::-1],
     "assets":[{"id":"spin","layers":pre}]}
json.dump(doc, open(sys.argv[1] if len(sys.argv) > 1 else "bench/lottie/mid.json", "w"))
