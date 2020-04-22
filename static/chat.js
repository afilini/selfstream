function DonationBadgeContainer(id) {
    const container = $('#' + id);

    let chat = null;

    this.addBadge = function(amount, duration, link) {
        let width = 0;
        let style = '';
        let textStyle = 'white';
        let bgColor = '';

        if (amount <= 1000) {
            width = 20;
            style = 'info';
            bgColor = 'rgb(47, 140, 155)';
        } else if (amount <= 10000) {
            width = 25;
            style = 'primary';
            bgColor = '#185eaa';
        } else if (amount <= 25000) {
            width = 40;
            style = 'success';
            bgColor = 'rgb(52, 155, 75)';
        } else if (amount <= 50000) {
            width = 60;
            style = 'warning';
            textStyle = 'dark';
            bgColor = 'rgb(240, 184, 16)';
        } else {
            width = 100;
            style = 'danger';
            bgColor = 'rgb(181, 47, 59)';
        }

        const item = $('<div class="progress position-relative mr-1 p-0" style="height: 4em;"></div>');
        item.css("width", width + "%");
        item.css("background-color", bgColor);

        item.click(function () {
            chat.scrollTo(link);
        });

        const progress = $(`<div class="progress-bar progress-bar-striped progress-bar-animated bg-${style}" role="progressbar" style="width: 100%;" aria-valuenow="10" aria-valuemin="0" aria-valuemax="100"></div>
                          <div class="justify-content-center align-self-center d-flex position-absolute w-100 text-truncate text-white">
                          <h5 class="m-0 font-weight-bold text-${textStyle}">${amount} <i class="fas fa-comment-dollar"></i></h5>
                          </div>`);
        item.append(progress);

        $(progress).animate({width: '0%'}, duration * 1000, 'linear', () => {
            item.remove();
        });

        container.append(item);

        return { style, textStyle };
    };

    this.setChat = function(chatRef) {
        chat = chatRef;
    }

    return this;
}

function DonateModal(modalId, openButtonId, sendButtonId, messageId, amountValueName, getInvoiceCb, chat) {
    const modal = $('#' + modalId);
    const openButton = $('#' + openButtonId);
    const sendButton = $('#' + sendButtonId);
    const textBox = $('#' + messageId);

    sendButton.click(function (e) {
        e.preventDefault();

        const value = $("input[name='" + amountValueName  + "']:checked").val();
        getInvoiceCb(parseInt(value), textBox.val(), function (id) {
            window.btcpay.showInvoice(id);
        });
    });

    openButton.click(function () {
        textBox.val(chat.getMessage());
    });

    modal.on('shown.bs.modal', function () {
        textBox.focus();
    });

    window.btcpay.onModalWillEnter(() => {
        modal.modal('hide');
    });

    window.btcpay.onModalWillLeave(() => {
        chat.clearMessage();
    });

    return this;
}

function Chat(id, textFieldId, sendButtonId, donationBadges) {
    donationBadges.setChat(this);

    const _self = this;

    this.connected = false;
    this.username = '';
    this.sendCb = () => {};

    const element = $('#' + id);
    const textField = $('#' + textFieldId);
    const sendButton = $('#' + sendButtonId);

    if (textField.val().length > 0) {
        sendButton.prop('disabled', false);
    }

    textField.on('keypress',function(e) {
        if(e.which == 13) {
            e.preventDefault();
            _self.sendMessage();  
        }
    });
    textField.on('change keyup paste', function () {
        if (textField.val().length > 0 && _self.connected) {
            sendButton.prop('disabled', false);
        } else {
            sendButton.prop('disabled', true);
        }
    });

    sendButton.click(function (e) {
        e.preventDefault();
        _self.sendMessage();
    });

    function replyTo(to) {
        textField.val(textField.val() + '@' + to + ' ');
        textField.focus();
    }

    this.getMessage = function () {
        return textField.val();
    }

    this.clearMessage = function () {
        textField.val('');
    }

    this.sendCustomMessage = function (msg) {
        if (!_self.connected) {
            return;
        }

        textField.val('');

        _self.sendCb(msg);
    }

    this.sendMessage = function () {
        if (!_self.connected) {
            return;
        }

        const msg = textField.val();
        textField.val('');

        _self.sendCb(msg);
    }

    this.addMessage = function (from, msg, extra) {
        let msg_author = $('<a href="#" class="author-name"></a>').text(from);
        msg_author.click(() => replyTo(msg_author.text()));
        let msg_item = $('<li class="list-group-item"></li>').text(": " + msg).prepend(msg_author);

        if (msg.includes("@" + _self.username)) {
            msg_item.addClass('font-weight-bold')
        }

        if (extra) {
            console.log(donationBadges);
            const { style, textStyle } = donationBadges.addBadge(extra.amount, extra.duration, msg_item);

            msg_item.addClass('bg-' + style).addClass('text-' + textStyle);
            msg_author.addClass('text-' + textStyle);

            msg_author.after(' +' + extra.amount + ' <i class="fas fa-comment-dollar"></i>')
        } else if (from == _self.username) {
            msg_item.addClass('bg-light');
        }

        element.append(msg_item);
    }

    this.scrollTo = function (item) {
        window.element = element;
        window.item = item;
        element.animate({
            scrollTop: item.offset().top - element.offset().top + element.scrollTop()
        });
    }

    this.atBottom = function () {
        return element.height() + element.scrollTop() >= element.prop("scrollHeight")
    }

    this.scrollBottom = function () {
        element.scrollTop(element.prop("scrollHeight"));
    }

    this.setUsername = function (username) {
        _self.username = username;
    }

    this.setSendCb = function (sendCb) {
        _self.sendCb = sendCb;
    }

    this.setConnected = function (connected) {
        if (!connected) {
            sendButton.prop('disabled', true);
            sendButton.html('<i class="fas fa-sync fa-spin"></i>');
        } else {
            if (textField.val().length > 0) {
                sendButton.prop('disabled', false);
            }

            sendButton.html('<i class="fas fa-paper-plane"></i>');
        }

        _self.connected = connected;
    }
    // start loading icon
    this.setConnected(false);

    return this;
}

function Socket(url, room, onClose, chat) {
    const socket = new WebSocket(url);

    let reqInvoiceCb = null;

    function send(method, data) {
        let obj = {};
        obj[method] = data;

        console.debug(obj);
        socket.send(JSON.stringify(obj));
    }

    this.getInvoice = function (amount, message, cb) {
        reqInvoiceCb = cb;
        send("GetInvoice", { amount, message });
    };

    socket.onopen = () => {
        send("Join", {room});
    };

    socket.onmessage = (msg) => {
        console.debug(msg.data);
        const data = JSON.parse(msg.data);

        if (data.AssignedUsername) {
            chat.setUsername(data.AssignedUsername.username);
            chat.setSendCb((m) => { send("ClientMessage", { message: m }) });
            chat.setConnected(true);
            
            chat.scrollBottom();
        } else if (data.ServerMessage) {
            const atBottom = chat.atBottom();
            chat.addMessage(data.ServerMessage.from, data.ServerMessage.message, data.ServerMessage.extra);

            if (atBottom) {
                chat.scrollBottom();
            }
        } else if (data.Invoice) {
            reqInvoiceCb(data.Invoice.id);
            reqInvoiceCb = null;
        } else if (data.UpdateViewers) {
            $('#viewers').text(data.UpdateViewers.viewers);
        }
    };

    socket.onclose = () => {
        chat.setConnected(false);
        onClose();
    };

    socket.onerror = (e) => {
        console.error(e);

        socket.close();
    };

    return this;
}

$(document).ready(function() {
    const donationBadges = new DonationBadgeContainer("donationBadgeContainer");
    const chat = new Chat("chatList", "chatText", "chatSendButton", donationBadges);
    const donateModal = new DonateModal("donateModal", "openDonateModalButton", "donateModalButton", "donateModalText", "amountValue", getInvoice, chat);
    let socket = null;

    const queryString = window.location.search;
    const urlParams = new URLSearchParams(queryString);

    function getInvoice(amount, message, cb) {
        socket.getInvoice(amount, message, cb);
    }

    function connectSocket() {
        const map = { "http:": "ws://", "https:": "ws://" };
        const url = map[window.location.protocol] + window.location.hostname + (window.location.port ? ":" + window.location.port : "") + "/ws";
        
        socket = new Socket(url, urlParams.get('v'), () => { setTimeout(connectSocket, 1000) }, chat);
    }
    connectSocket();
});
