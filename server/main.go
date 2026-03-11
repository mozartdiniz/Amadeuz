package main

import (
	"log"
	"net/http"

	"github.com/gorilla/websocket"
)

var upgrader = websocket.Upgrader{
	// Allow any origin — clients are native apps, not browsers.
	CheckOrigin: func(r *http.Request) bool { return true },
}

func main() {
	s := newStore("data.json")
	h := newHub(s)
	bs := newBlobStore("blobs")

	http.HandleFunc("/blobs/", bs.HandleBlob)

	http.HandleFunc("/ws", func(w http.ResponseWriter, r *http.Request) {
		conn, err := upgrader.Upgrade(w, r, nil)
		if err != nil {
			log.Println("upgrade error:", err)
			return
		}
		defer func() {
			h.remove(conn)
			conn.Close()
			log.Printf("client disconnected, total: %d", len(h.clients))
		}()

		h.add(conn)
		log.Printf("client connected, total: %d", len(h.clients))

		// Send full state to the new client.
		h.sendInit(conn)

		for {
			var msg Msg
			if err := conn.ReadJSON(&msg); err != nil {
				break
			}
			h.handle(conn, msg)
		}
	})

	addr := ":8080"
	log.Printf("amadeuz server listening on %s", addr)
	log.Fatal(http.ListenAndServe(addr, nil))
}
