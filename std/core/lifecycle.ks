# core.lifecycle — Drop, Copy, Dupe protocols

type Drop {
    func drop(self)
}

type Copy {
    func copy(self): Self
}

type Dupe {
    func dupe(self): Self
}
