# core.indexing — GetItem[K, V] and SetItem[K, V] protocols

type GetItem[K, V] {
    func get_item(self, key: K): V
}

type SetItem[K, V] {
    func set_item(self, key: K, val: V)
}
