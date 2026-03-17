kind Rect { w: Int, h: Int }

impl Rect {
    func area(self): Int {
        ret self.w * self.h
    }
}

let r = Rect { w: 3, h: 7 }
print(r.area())